#![cfg(unix)]

use std::time::Duration;

use ptytest::{CommandSpec, ExitStatus, PtyTest, PtyTestError, Scenario, TestEnv};

#[test]
fn cli_rejects_missing_source_on_a_terminal() -> Result<(), PtyTestError> {
    let scenario = Scenario::new("defuddle-no-terminal-input")?
        .command(CommandSpec::new(env!("CARGO_BIN_EXE_defuddle")))
        .environment(TestEnv::hermetic_ascii()?);
    let mut terminal = PtyTest::spawn(scenario)?;
    let deadline = terminal.deadline(Duration::from_secs(5));

    terminal.wait_for_screen(deadline, "missing input diagnostic", |screen| {
        screen.contains("No input provided. Pass an HTML file path")
    })?;
    assert_eq!(terminal.wait_for_exit(deadline)?, ExitStatus::Code(1));
    assert_eq!(terminal.finish(deadline)?, ExitStatus::Code(1));
    Ok(())
}
