use serde::{Deserialize, Serialize};

use crate::domain::greeting::Greeting;

#[derive(Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct HelloResponse {
    pub message: String,
}

impl From<Greeting> for HelloResponse {
    fn from(greeting: Greeting) -> Self {
        Self {
            message: greeting.into_message(),
        }
    }
}
