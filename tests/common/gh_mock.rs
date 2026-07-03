use anyhow::Result;
use ghview::data::GhRunner;

pub struct MockGh {
    responses: Vec<(String, Result<String, String>)>,
}

impl MockGh {
    pub fn new() -> Self {
        Self {
            responses: Vec::new(),
        }
    }

    pub fn on(mut self, endpoint: &str, body: &str) -> Self {
        self.responses
            .push((endpoint.to_string(), Ok(body.to_string())));
        self
    }

    pub fn on_err(mut self, endpoint: &str, stderr: &str) -> Self {
        self.responses
            .push((endpoint.to_string(), Err(stderr.to_string())));
        self
    }

    pub fn on_fixture(self, endpoint: &str, fixture_name: &str) -> Self {
        let body = super::fixture(fixture_name);
        self.on(endpoint, &body)
    }
}

impl GhRunner for MockGh {
    async fn run(&self, args: &[&str]) -> Result<String> {
        let key = if args.first() == Some(&"api") {
            args.get(1).copied().unwrap_or_default().to_string()
        } else {
            args.join(" ")
        };

        for (k, result) in &self.responses {
            if k == &key {
                return result.clone().map_err(|e| anyhow::anyhow!(e));
            }
        }

        panic!("MockGh: unregistered gh call: {args:?}")
    }
}
