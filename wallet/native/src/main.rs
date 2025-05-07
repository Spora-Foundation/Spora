use tondi_cli_lib::{tondi_cli, TerminalOptions};

#[tokio::main]
async fn main() {
    let result = tondi_cli(TerminalOptions::new().with_prompt("$ "), None).await;
    if let Err(err) = result {
        println!("{err}");
    }
}
