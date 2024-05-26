fn main() {
    println!("Hello, world!");
}

#[cfg(test)]
mod tests {
    use loadbalancer::Loadbalancer;
    use loadbalancer::RoundRobinServerPolicy;
    use loadbalancer::SingleServerPolicy;
    use reqwest::{Client, StatusCode};
    use wiremock::{matchers::method, Mock, MockServer, ResponseTemplate};
    #[tokio::test]
    async fn test_get_root() {
        // Setup a mock upstream server, to test
        // that the request gets forwarded to it
        let mock_server = MockServer::start().await;
        Mock::given(method("GET"))
            .respond_with(ResponseTemplate::new(200).set_body_string("backend"))
            .expect(1)
            .mount(&mock_server)
            .await;

        let client = Client::new();
        println!("Mock server uri : {}", mock_server.uri());
        let policy = Box::new(SingleServerPolicy::new(mock_server.uri().clone()));

        // The class under test, the load balancer itself
        let server = Loadbalancer::new(8080, policy);
        let server_uri = server.uri();
        tokio::spawn(async move { server.run().await });

        // Wait for the server to be up (will fix this later)
        wait_server_up(&client, &server_uri, 3).await;
        // Check that we receive response from the mock backend
        // (and not from the load balancer)
        let response = client.get(server_uri).send().await.unwrap();
        assert_eq!(StatusCode::OK, response.status());
        assert_eq!("backend", response.text().await.unwrap());
    }

    #[tokio::test]
    async fn test_get_round_robin_policy() {
        let mocks = [
            MockServer::start().await,
            MockServer::start().await,
            MockServer::start().await,
        ];

        Mock::given(method("GET"))
            .respond_with(ResponseTemplate::new(200).set_body_string("1"))
            .mount(&mocks[0])
            .await;

        Mock::given(method("GET"))
            .respond_with(ResponseTemplate::new(200).set_body_string("2"))
            .mount(&mocks[1])
            .await;

        Mock::given(method("GET"))
            .respond_with(ResponseTemplate::new(200).set_body_string("3"))
            .mount(&mocks[2])
            .await;

        let client = Client::new();
        let mock_uris: Vec<_> = mocks.iter().map(|mock| mock.uri()).collect();

        let policy = Box::new(RoundRobinServerPolicy::new(mock_uris.clone()));

        // The class under test, the load balancer itself
        let server = Loadbalancer::new(8082, policy);
        let server_uri = server.uri();
        tokio::spawn(async move { server.run().await });

        // Wait for the server to be up (will fix this later)
        wait_server_up(&client, &server_uri, 3).await;
        // Check that we receive response from the mock backend
        // (and not from the load balancer)
        let response = client.get(&server_uri).send().await.unwrap();
        assert_eq!(StatusCode::OK, response.status());
        assert_eq!("1", response.text().await.unwrap());

        let response = client.get(&server_uri).send().await.unwrap();
        assert_eq!(StatusCode::OK, response.status());
        assert_eq!("2", response.text().await.unwrap());
    }

    pub async fn wait_server_up(client: &Client, uri: &str, max_retries: usize) {
        let health_uri = format!("{}/health", uri);
        for _ in 0..max_retries {
            let response = client.get(&health_uri).send().await;
            if response.is_ok() {
                return;
            }
            tokio::time::sleep(std::time::Duration::from_secs(1)).await;
        }
        panic!("Server didn't start...");
    }
}
