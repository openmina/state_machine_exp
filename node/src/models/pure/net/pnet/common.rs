use crate::automaton::state::Uid;
use salsa20::{
    cipher::{generic_array::GenericArray, KeyIvInit, StreamCipherSeek},
    XSalsa20, XSalsaCore,
};
use serde::{ser::SerializeTuple, Serializer};
use serde::{Deserialize, Deserializer, Serialize};
use std::{
    fmt,
    ops::{Deref, DerefMut},
};

#[derive(Debug)]
pub struct PnetKey(pub [u8; 32]);

impl PnetKey {
    pub fn new(chain_id: &str) -> Self {
        use blake2::{
            digest::{generic_array, Update, VariableOutput},
            Blake2bVar,
        };

        let mut key = generic_array::GenericArray::default();
        Blake2bVar::new(32)
            .expect("valid constant")
            .chain(b"/coda/0.0.1/")
            .chain(chain_id.as_bytes())
            .finalize_variable(&mut key)
            .expect("good buffer size");
        Self(key.into())
    }
}

#[derive(Clone)]
pub struct XSalsa20Wrapper {
    inner: XSalsa20,
}

impl Serialize for XSalsa20Wrapper {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let mut state = serializer.serialize_tuple(2)?;
        state.serialize_element(&self.inner.get_core())?;
        state.serialize_element(&self.inner.try_current_pos::<usize>().unwrap())?;
        state.end()
    }
}

impl<'de> Deserialize<'de> for XSalsa20Wrapper {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let (core, pos): (XSalsaCore<salsa20::cipher::consts::U10>, usize) =
            Deserialize::deserialize(deserializer)?;

        let mut inner = XSalsa20::from_core(core);
        inner.try_seek(pos).unwrap();

        Ok(XSalsa20Wrapper { inner })
    }
}

impl XSalsa20Wrapper {
    pub fn new(shared_secret: &[u8; 32], nonce: &[u8; 24]) -> Self {
        XSalsa20Wrapper {
            inner: XSalsa20::new(
                GenericArray::from_slice(shared_secret),
                GenericArray::from_slice(nonce),
            ),
        }
    }
}

impl Deref for XSalsa20Wrapper {
    type Target = XSalsa20;

    fn deref(&self) -> &Self::Target {
        &self.inner
    }
}

impl DerefMut for XSalsa20Wrapper {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.inner
    }
}

impl fmt::Debug for XSalsa20Wrapper {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("XSalsa20").finish()
    }
}

#[derive(Debug)]
pub enum ConnectionState {
    Init,
    NonceSent {
        send_request: Uid,
        nonce: [u8; 24],
    },
    NonceWait {
        recv_request: Uid,
        nonce_sent: [u8; 24],
    },
    Ready {
        send_cipher: XSalsa20Wrapper,
        recv_cipher: XSalsa20Wrapper,
    },
}
