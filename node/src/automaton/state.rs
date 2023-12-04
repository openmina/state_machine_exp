use std::collections::BTreeMap;

use rand::rngs::SmallRng;

// An Uid is a simple way to implement descriptors to reference object between different Models.
//
// It should be possible to re-use the same Uid value for a new object if the previous object
// using it was freed. However, there is no practical need for this "optimization". To keep it
// simple we just use a 64-bit counter (practically unfeasible to overflow) that provide unique
// values during the program's life-time.
#[derive(PartialEq, PartialOrd, Eq, Ord, Clone, Copy, Debug)]
pub struct Uid(u64);

impl Default for Uid {
    fn default() -> Self {
        Uid(0)
    }
}

impl From<Uid> for usize {
    fn from(item: Uid) -> usize {
        item.0 as usize
    }
}

impl From<usize> for Uid {
    fn from(item: usize) -> Self {
        Uid(item as u64)
    }
}

impl From<Uid> for u64 {
    fn from(item: Uid) -> u64 {
        item.0
    }
}

impl From<u64> for Uid {
    fn from(item: u64) -> Self {
        Uid(item)
    }
}

impl Uid {
    pub fn next(&mut self) -> Uid {
        let ret = Uid(self.0);
        self.0 = self.0.wrapping_add(1);
        assert_ne!(self.0, 0);
        ret
    }
}

// Models usually need to keep multiple object instances that can be referenced by Uids.
pub type Objects<T> = BTreeMap<Uid, T>;

pub trait ModelState {
    fn state<T>(&self) -> &T;
    fn state_mut<T>(&mut self) -> &mut T;
}

pub struct State<Substates: ModelState> {
    // all models should generate Uids from this source
    pub uid_source: Uid,
    // If your model requires a cryptographically secure RNG then use `models::efectful::rng::RngAction`.
    // Otherwise, use this RNG which is faster and deterministic.
    pub rng: SmallRng,
    // All the (Pure/Input)Models' states used in the state-machine configuration.
    // OutputModels' state are not part of the state-machine state.
    pub models: Substates,
}

impl<Substates: ModelState> State<Substates> {
    pub fn new_uid(&mut self) -> Uid {
        self.uid_source.next()
    }

    pub fn substate<T>(&self) -> &T {
        self.models.state()
    }

    pub fn substate_mut<T>(&mut self) -> &mut T {
        self.models.state_mut()
    }
}
