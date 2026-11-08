use std::net::TcpListener;
use std::time::Duration;

fn main() -> std::io::Result<()> {
    let address = std::env::args()
        .nth(1)
        .unwrap_or_else(|| "127.0.0.1:8787".to_owned());
    let listener = TcpListener::bind(&address)?;
    println!("encore mock room server on http://{address}");
    encore_room::mock::serve(listener, Duration::from_secs(25));
    Ok(())
}
