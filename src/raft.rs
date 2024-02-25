use std::collections::HashMap;

use std::time::Duration;
use tokio::time::sleep;

enum NodeType {
    Leader,
    Follower,
    Candidate,
}

type NodeID = String;

type DataStore = HashMap<String, String>;

struct SetCommand {
    key: String,
    value: String,
}

fn step(store: &mut DataStore, command: &SetCommand) {
    store.insert(command.key.clone(), command.value.clone());
}

struct LogEntry {
    term: usize,
    command: SetCommand,
}

struct LogEntryithIndex {
    index: usize,
    entry: LogEntry,
}

struct Node<Sleep> {
    node_type: NodeType,
    term: usize,
    voted_for: Option<NodeID>,
    log: Vec<LogEntry>,
    data_store: DataStore,
    commit_index: usize,
    sleep: Sleep,
}

struct AppendEntriesRPCRequest {
    term: usize,
    leader_id: NodeID,
    prev_log_index: usize,
    prev_log_term: usize,
    entries: Vec<LogEntryithIndex>,
    leader_commit: usize,
}

struct AppendEntriesRPCResponse {
    term: usize,
    success: bool,
}

impl<Sleep> Node<Sleep>
where
    Sleep: Fn(Duration) -> (),
{
    fn new(sleep: Sleep) -> Self {
        Node {
            node_type: NodeType::Follower,
            term: 0,
            voted_for: None,
            log: Vec::new(),
            data_store: DataStore::new(),
            commit_index: 0,
            sleep,
        }
    }

    fn listen_for_leader_heartbeat(&mut self) {
        loop {
            (self.sleep)(Duration::from_millis(100));
        }
    }

    fn append_entries(&mut self, request: AppendEntriesRPCRequest) -> AppendEntriesRPCResponse {
        let mut success = true;

        if request.term < self.term {
            success = false;
        }

        if let Some(entry) = self.log.get(request.prev_log_index) {
            if entry.term != request.prev_log_term {
                success = false;
            }
        }

        if success {
            if request.leader_commit > self.commit_index {
                // TODO: we are assuming that the entries are in order
                let last_new_entry_index = request.entries.last().map_or(0, |e| e.index);
                self.commit_index = std::cmp::min(request.leader_commit, last_new_entry_index);
            }

            for LogEntryithIndex { index, entry } in request.entries {
                let current = self.log.get_mut(index);
                match current {
                    Some(current) => {
                        if current.term != entry.term {
                            // TODO: we are assuming that the entries are in order
                            self.log.truncate(index);
                            self.log.push(entry);
                        }
                    }
                    None => {
                        // TODO: we are assuming that the entries are in order
                        self.log.push(entry);
                    }
                };
            }
        }

        AppendEntriesRPCResponse {
            term: self.term,
            success,
        }
    }
}
