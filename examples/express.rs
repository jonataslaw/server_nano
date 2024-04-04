use server_nano::{json, Server};

fn main() {
    let mut app = Server::new();

    app.get("/", |_, res| res.send("welcome to home page!"));

    app.get("/user/:id", |req, res| {
        let user_id = req.parameter("id").unwrap();
        let json_value = json!({ "username": user_id });
        res.json(&json_value)
    });

    app.get("/product/:name", |req, res| {
        let product_name = req.parameter("name").unwrap();
        let message = &format!("Welcome to product page of product: {}", product_name);
        res.send(message)
    });

    app.post("/test", |_, res| res.send("test!"));

    app.post("/settings", |req, res| {
        let json_body = req.json_body().unwrap();

        let response = json!({
            "success": true,
            "message": "Settings updated successfully",
            "body": json_body
        });
        res.json(&response)
    });

    app.listen("127.0.0.1:8080").unwrap();
}
