use jiff::Timestamp;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Clone, Copy, Debug, Deserialize, Serialize)]
pub struct Header {
    pub id: Uuid,
    pub time_created: Timestamp,
    pub time_last_edited: Timestamp,
}

impl Header {
    pub fn new() -> Header {
        let now = Timestamp::now();
        Header {
            id: Uuid::now_v7(),
            time_created: now,
            time_last_edited: now,
        }
    }

    pub fn update_edited(mut self) -> Header {
        self.time_last_edited = Timestamp::now();
        self
    }
}
