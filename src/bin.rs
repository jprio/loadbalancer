fn main() {
    println!("Hello, world!");
}

#[cfg(test)]
mod tests {
    use loadbalancer::Loadbalancer;
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
        // The class under test, the load balancer itself
        let server = Loadbalancer::new(8080, vec![mock_server.uri()]);
        let server_uri = server.uri();
        tokio::spawn(async move { server.run().await });

        // Wait for the server to be up (will fix this later)
        tokio::time::sleep(std::time::Duration::from_secs(3)).await;

        // Check that we receive response from the mock backend
        // (and not from the load balancer)
        let response = client.get(server_uri).send().await.unwrap();
        assert_eq!(StatusCode::OK, response.status());
        assert_eq!("backend", response.text().await.unwrap());
    }
}
