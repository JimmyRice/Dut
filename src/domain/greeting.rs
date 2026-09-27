#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Greeting {
    message: String,
}

impl Greeting {
    pub fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
        }
    }

    pub fn message(&self) -> &str {
        &self.message
    }

    pub fn into_message(self) -> String {
        self.message
    }
}
