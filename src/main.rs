use anyhow::Context;
use log::{debug, LevelFilter, SetLoggerError};
use std::env;
use std::fs::File;
use std::io::{copy, BufRead, BufReader, Error, Read, Seek};
use std::os::unix::prelude::CommandExt;
use std::process::Command;

mod build {
    include!(concat!(env!("OUT_DIR"), "/built.rs"));
}

#[cfg(test)]
mod tests {
    use indoc::indoc;
    use std::io::{BufRead, Cursor};

    fn buf<T>(content: T) -> (Vec<String>, Vec<String>)
    where
        T: AsRef<[u8]>,
    {
        let buf = Cursor::new(content);

        let (left, right) = crate::split(buf).unwrap();
        let right = right.lines().map(Result::unwrap).collect::<Vec<String>>();

        (left, right)
    }

    #[test]
    fn empty() {
        let res = buf(indoc! {r"
        "});

        let left = res.0;
        let right = res.1;

        assert!(left.is_empty());
        assert!(right.is_empty());
    }

    #[test]
    fn trim() {
        let res = buf(indoc! {r"
        #! hello\
        #               my-command foo bar
            This is the body
        "});

        let left = res.0.join(" ");
        let right = res.1.join(" ");

        assert_eq!("my-command foo bar", left);
        assert_eq!("    This is the body", right);
    }

    #[test]
    fn no_head() {
        let res = buf(indoc! {r"
        #! hello\
        This is the body
        "});

        let left = res.0;
        let right = res.1.join(" ");

        assert!(left.is_empty());
        assert_eq!("This is the body", right);
    }

    #[test]
    fn no_tail() {
        let res = buf(indoc! {r"
        #! hello\
        # my-command foo bar
        #   wee
        "});

        let left = res.0.join(" ");
        let right = res.1;

        assert_eq!("my-command foo bar wee", left);
        assert!(right.is_empty());
    }

    #[test]
    fn multi_head() {
        let res = buf(indoc! {r"
        #! hello\
        # my-command foo bar
        #   wee
        This is the body
        "});

        let left = res.0.join(" ");
        let right = res.1.join(" ");

        assert_eq!("my-command foo bar wee", left);
        assert_eq!("This is the body", right);
    }

    #[test]
    fn multi_tail() {
        let res = buf(indoc! {r"
        #! hello
        # my-command foo bar
        This is the body
        This is another part of it
    "});

        let left = res.0.join(" ");
        let right = res.1.join(" ");

        assert_eq!("my-command foo bar", left);
        assert_eq!("This is the body This is another part of it", right);
    }

    #[test]
    fn multi_head_tail() {
        let res = buf(indoc! {r"
        #! hello\
        # my-command foo bar
        #   wee
        This is the body
        This is another part of it
        "});

        let left = res.0.join(" ");
        let right = res.1.join(" ");

        assert_eq!("my-command foo bar wee", left);
        assert_eq!("This is the body This is another part of it", right);
    }
}

fn split<R: Read + Seek>(reader: R) -> Result<(Vec<String>, BufReader<R>), Error> {
    let comment = "#";

    let mut head = Vec::new();
    let mut r = BufReader::new(reader);

    for line in r.by_ref().lines().skip(1) {
        let line = line?;
        let n: i64 = line.len() as i64;

        if line.starts_with(comment) {
            let line = line
                .trim_start_matches(comment)
                .trim_start_matches(char::is_whitespace);
            head.push(line.to_string());
        } else {
            let delta = (n + 1) * -1; // negative len of line + newline
            r.seek_relative(delta)?;
            break;
        }
    }

    Ok((head, r))
}

struct Config {
    env_for_tail: String,
    keep: bool,
    verbose: bool,
}

fn config() -> anyhow::Result<(Config, Vec<String>)> {
    use clap::{app_from_crate, AppSettings, Arg};
    include_str!("../Cargo.toml");

    let version = build::PKG_VERSION.to_owned() + " (" + build::GIT_VERSION.unwrap() + ")";

    let matches = app_from_crate!()
        .long_version(version.as_ref())
        .setting(AppSettings::TrailingVarArg)
        .arg("-k, --keep 'Keep the temporary tail file'")
        .arg(
            Arg::from(
                "-t --env-for-tail 'Environment variable pointing to the temporary tail file'",
            )
            .default_value("RUNT_TAIL"),
        )
        .arg("-v, --verbose 'Enable verbose logging'")
        .arg("<cmd>... Additional arguments for the script")
        .try_get_matches()?;

    let head_args = matches
        .values_of("cmd")
        .context("Failed to collect file varargs")?
        .skip(1)
        .map(String::from)
        .collect::<Vec<String>>();

    Ok((
        Config {
            env_for_tail: matches.value_of("env-for-tail").unwrap().into(),
            keep: matches.is_present("keep"),
            verbose: matches.is_present("verbose"),
        },
        head_args,
    ))
}

fn init_logger(config: &Config) -> Result<(), SetLoggerError> {
    let mut logger = env_logger::builder();
    logger.format_timestamp(None);

    if config.verbose {
        logger.filter_level(LevelFilter::Debug);
    }

    logger.try_init()
}

fn main() -> anyhow::Result<()> {
    let (config, head_args) = config()?;
    init_logger(&config)?;

    let caller =
        env::var("_").context(r#"Failed to retrieve file reference from environment via "_""#)?;
    let result;

    debug!("file is '{}'", caller);

    {
        let file =
            File::open(&caller).with_context(|| format!("Failed to open file {}", &caller))?;
        result = split(file).context("Failed to split file into head and tail")?;
    }

    let head = result.0.join(" ");

    debug!("head is {}", head);
    debug!("head argv is {:?}", head_args);

    let mut tail = result.1;

    let mut file = tempfile::Builder::new()
        .prefix("runt")
        .rand_bytes(16)
        .tempfile()
        .context("Failed to create temporary file for tail")?;

    let path: String = file
        .path()
        .to_str()
        .context("Failed to convert temporary file for tail to path")?
        .into();

    debug!("tail is at {}", path);

    let fd = file.as_file_mut();

    copy(&mut tail, fd).with_context(|| {
        format!(
            "Failed to copy template file for template body at {:?}",
            &file.path()
        )
    })?;

    if config.keep {
        file.keep().with_context(|| {
            format!(
                "Failed to keep template file for template body at {:?}",
                path
            )
        })?;
    }

    let head = head + &head_args.join(" ");

    Command::new("sh")
        .arg("-c")
        .arg(head)
        .env(config.env_for_tail, path)
        .exec();

    Ok(())
}
