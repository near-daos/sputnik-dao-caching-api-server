#[rocket::main]
async fn main() {
    sputnik_indexer::rocket()
        .launch()
        .await
        .expect("server failed to launch");
}
