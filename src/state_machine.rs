use std::{collections::HashMap, hash::Hash};

pub(crate) trait StateMachine {
    type State;
    type Event;
    fn get_state(&self) -> &Self::State;
    fn apply(&mut self, event: Self::Event);
}

pub(crate) enum HashMapStateMachineEvent<K, V> {
    Put(K, V),
    Delete(K),
}

impl<K, V> StateMachine for HashMap<K, V>
where
    K: Hash + Eq,
{
    type Event = HashMapStateMachineEvent<K, V>;
    type State = HashMap<K, V>;
    fn get_state(&self) -> &Self {
        self
    }

    fn apply(&mut self, event: HashMapStateMachineEvent<K, V>) {
        match event {
            HashMapStateMachineEvent::Put(k, v) => {
                self.insert(k, v);
            }
            HashMapStateMachineEvent::Delete(k) => {
                self.remove(&k);
            }
        }
    }
}
