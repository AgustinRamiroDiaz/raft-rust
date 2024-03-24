use crate::server::main_grpc::Entry;
use serde::Serialize;

#[derive(Debug, Clone)]
pub(crate) struct LogEntry<T> {
    pub(crate) term: u64,
    pub(crate) index: u64,
    pub(crate) command: T,
}

// TODO: change for TryInto and TryFrom
impl<T> From<LogEntry<T>> for Entry
where
    T: Serialize,
{
    fn from(val: LogEntry<T>) -> Self {
        Entry {
            term: val.term,
            index: val.index,
            command: serde_json::to_string(&val.command).unwrap(),
        }
    }
}

impl<T> From<Entry> for LogEntry<T>
where
    T: for<'de> serde::Deserialize<'de>,
{
    fn from(entry: Entry) -> Self {
        Self {
            term: entry.term,
            index: entry.index,
            command: serde_json::from_str(&entry.command).unwrap(),
        }
    }
}
