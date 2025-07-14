use super::git_passthrough::git_passthrough;

pub fn handle_status(args: &[String]) -> Result<(), Box<dyn std::error::Error>> {
    git_passthrough("status", args)
}
