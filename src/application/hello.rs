use crate::domain::greeting::Greeting;

const HELLO_MESSAGE: &str = "Hello, World!";

#[derive(Clone, Copy, Debug, Default)]
pub struct HelloService;

impl HelloService {
    pub fn greeting(&self) -> Greeting {
        Greeting::new(HELLO_MESSAGE)
    }
}
