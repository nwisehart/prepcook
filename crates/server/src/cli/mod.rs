//! Command-line parsing for the `prepcook` binary. With no command it starts the
//! server; `up`, `down` and `status` manage the board stack in Docker
//! ([`up`]); `secret` manages the secrets store; `integrations` describes the
//! integration providers.

pub mod up;

use std::path::PathBuf;

pub const USAGE: &str = "\
usage: prepcook [command]

With no command, prepcook starts the server against the board named by
PREPCOOK_SHARED_API_BASE.

commands:
  up [--no-browser] [--board-only] [--compose <file>]
                 start the board in Docker, wait for it, then start the server
  down [--volumes] [--compose <file>]
                 stop the board; --volumes also deletes its data
  status         say whether the board answers, which port the server is on
                 and whether the board accepts this machine's board secret
  secret         manage the secrets store (prepcook secret, for its usage)
  integrations describe [--json]
                 list every integration provider and its settings
  help           print this message";

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Command {
    Serve,
    Up(UpOptions),
    Down(DownOptions),
    Status,
    Secret(Vec<String>),
    /// `integrations describe`; `true` prints JSON.
    IntegrationsDescribe {
        json: bool,
    },
    Help,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct UpOptions {
    pub no_browser: bool,
    pub board_only: bool,
    /// A compose file to start instead of the embedded release one.
    pub compose: Option<PathBuf>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct DownOptions {
    pub volumes: bool,
    pub compose: Option<PathBuf>,
}

/// Parses the arguments after the program name. The error is a message for
/// the user; the caller prints it with [`USAGE`].
pub fn parse(args: &[String]) -> Result<Command, String> {
    let Some((command, rest)) = args.split_first() else {
        return Ok(Command::Serve);
    };
    match command.as_str() {
        "secret" => Ok(Command::Secret(rest.to_vec())),
        "integrations" => match rest.iter().map(String::as_str).collect::<Vec<_>>()[..] {
            ["describe"] => Ok(Command::IntegrationsDescribe { json: false }),
            ["describe", "--json"] => Ok(Command::IntegrationsDescribe { json: true }),
            _ => Err("usage: prepcook integrations describe [--json]".to_string()),
        },
        "help" | "--help" | "-h" => Ok(Command::Help),
        "status" => match rest.first() {
            None => Ok(Command::Status),
            Some(arg) => Err(format!("prepcook status takes no arguments (got {arg})")),
        },
        "up" => {
            let mut options = UpOptions::default();
            let mut args = rest.iter();
            while let Some(arg) = args.next() {
                match arg.as_str() {
                    "--no-browser" => options.no_browser = true,
                    "--board-only" => options.board_only = true,
                    _ => options.compose = Some(compose_flag(arg, &mut args, "up")?),
                }
            }
            Ok(Command::Up(options))
        }
        "down" => {
            let mut options = DownOptions::default();
            let mut args = rest.iter();
            while let Some(arg) = args.next() {
                match arg.as_str() {
                    "--volumes" | "-v" => options.volumes = true,
                    _ => options.compose = Some(compose_flag(arg, &mut args, "down")?),
                }
            }
            Ok(Command::Down(options))
        }
        other => Err(format!("unknown command: {other}")),
    }
}

/// `--compose <file>` or `--compose=<file>`; any other argument is an error.
fn compose_flag<'a>(
    arg: &str,
    rest: &mut impl Iterator<Item = &'a String>,
    command: &str,
) -> Result<PathBuf, String> {
    if arg == "--compose" {
        return rest
            .next()
            .map(PathBuf::from)
            .ok_or_else(|| "--compose needs a file".to_string());
    }
    if let Some(path) = arg.strip_prefix("--compose=")
        && !path.is_empty()
    {
        return Ok(PathBuf::from(path));
    }
    Err(format!("unknown option for prepcook {command}: {arg}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse_strs(args: &[&str]) -> Result<Command, String> {
        let args: Vec<String> = args.iter().map(|a| a.to_string()).collect();
        parse(&args)
    }

    #[test]
    fn no_command_starts_the_server() {
        assert_eq!(parse_strs(&[]), Ok(Command::Serve));
    }

    #[test]
    fn secret_keeps_its_own_arguments() {
        assert_eq!(
            parse_strs(&["secret", "set", "X"]),
            Ok(Command::Secret(vec!["set".into(), "X".into()]))
        );
    }

    #[test]
    fn integrations_describe_takes_an_optional_json_flag() {
        assert_eq!(
            parse_strs(&["integrations", "describe"]),
            Ok(Command::IntegrationsDescribe { json: false })
        );
        assert_eq!(
            parse_strs(&["integrations", "describe", "--json"]),
            Ok(Command::IntegrationsDescribe { json: true })
        );
        assert!(parse_strs(&["integrations"]).is_err());
        assert!(parse_strs(&["integrations", "describe", "--yaml"]).is_err());
    }

    #[test]
    fn up_flags() {
        assert_eq!(parse_strs(&["up"]), Ok(Command::Up(UpOptions::default())));
        assert_eq!(
            parse_strs(&["up", "--no-browser", "--board-only", "--compose", "a.yml"]),
            Ok(Command::Up(UpOptions {
                no_browser: true,
                board_only: true,
                compose: Some(PathBuf::from("a.yml")),
            }))
        );
        assert_eq!(
            parse_strs(&["up", "--compose=b.yml"]),
            Ok(Command::Up(UpOptions {
                compose: Some(PathBuf::from("b.yml")),
                ..UpOptions::default()
            }))
        );
        assert!(parse_strs(&["up", "--compose"]).is_err());
        assert!(parse_strs(&["up", "--compose="]).is_err());
        assert!(parse_strs(&["up", "--detach"]).is_err());
    }

    #[test]
    fn down_flags() {
        assert_eq!(
            parse_strs(&["down"]),
            Ok(Command::Down(DownOptions::default()))
        );
        assert_eq!(
            parse_strs(&["down", "--volumes", "--compose", "c.yml"]),
            Ok(Command::Down(DownOptions {
                volumes: true,
                compose: Some(PathBuf::from("c.yml")),
            }))
        );
        assert!(parse_strs(&["down", "--force"]).is_err());
    }

    #[test]
    fn help_status_and_unknown_commands() {
        assert_eq!(parse_strs(&["--help"]), Ok(Command::Help));
        assert_eq!(parse_strs(&["help"]), Ok(Command::Help));
        assert_eq!(parse_strs(&["status"]), Ok(Command::Status));
        assert!(parse_strs(&["status", "--json"]).is_err());
        assert!(parse_strs(&["serve"]).is_err());
    }
}
