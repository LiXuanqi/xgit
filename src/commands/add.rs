use std::process::Command;

pub fn handle_add(args: &[String]) -> Result<(), Box<dyn std::error::Error>> {
    let mut cmd = Command::new("git");
    cmd.arg("add");
    cmd.args(args);

    match cmd.status() {
        Ok(status) => {
            if !status.success() {
                std::process::exit(status.code().unwrap_or(1));
            }
        }
        Err(e) => {
            eprintln!("Error running git add: {e}");
            std::process::exit(1);
        }
    }

    Ok(())
}
