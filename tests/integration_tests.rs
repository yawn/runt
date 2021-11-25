use assert_cmd::{crate_name, Command};
use assert_fs::prelude::*;
use indoc::formatdoc;
use lazy_static::lazy_static;
use predicates::prelude::predicate;
use std::mem::forget;
use std::os::unix::fs::PermissionsExt;
use std::{env, fs};

lazy_static! {
    static ref MAIN: escargot::CargoRun = escargot::CargoBuild::new()
        .bin(crate_name!())
        .current_release()
        .current_target()
        .run()
        .unwrap();
}

#[test]
fn test_noop() {
    let mut main = Command::new(MAIN.path());
    main.assert()
        .stderr(predicate::str::contains(
            "following required arguments were not provided",
        ))
        .failure();
}

#[test]
fn test_simple() {
    let dir = assert_fs::TempDir::new().unwrap();

    // shebang lines don't support whitespace - let's create a symlink in the temp dir to prevent this
    let binary = dir.child("binary");
    binary.symlink_to_file(MAIN.path()).unwrap();

    let target = dir.child("target");

    let content = formatdoc! {r"
            #! {binary}
            # cat $RUNT_TAIL | awk '{{print toupper($0)}}' > {target}
            this is uppercase!
            ", binary=binary.display(), target=target.display()
    };

    let script = dir.child("script");
    script.write_str(&content).unwrap();

    let mut perms = fs::metadata(&script).unwrap().permissions();
    perms.set_mode(0o777);
    fs::set_permissions(&script, perms).unwrap();

    let ci = env::var_os("CI").is_some();

    if ci {
        println!("keeping temporary dir alive at {}", &dir.display());
        forget(dir);
    }

    let mut main = Command::new("/bin/bash");
    main.arg("-c").arg(script.path().as_os_str());
    main.assert().success();

    target.assert(predicate::eq("THIS IS UPPERCASE!\n"));
}
