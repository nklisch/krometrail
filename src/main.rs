mod app;
mod cli;

use std::process::ExitCode;

use clap::{CommandFactory, Parser};
use krometrail_core::{KrometrailError, RetryAdvice};

use app::build_runtime;
use cli::Cli;

const FAILURE_EXIT_CODE: u8 = 1;

fn main() -> ExitCode {
    let cli = match Cli::try_parse() {
        Ok(cli) => cli,
        Err(error) => error.exit(),
    };

    let Some(command) = cli.command else {
        let mut command = Cli::command();
        if let Err(error) = command.print_help() {
            eprintln!("failed to print help: {error}");
            return ExitCode::from(FAILURE_EXIT_CODE);
        }
        println!();
        return ExitCode::SUCCESS;
    };

    let executor = match tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
    {
        Ok(executor) => executor,
        Err(error) => {
            eprintln!("error[internal]: unable to initialize async runtime: {error}");
            return ExitCode::from(FAILURE_EXIT_CODE);
        }
    };

    match executor.block_on(build_runtime().run(command)) {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => report_error(&error),
    }
}

fn report_error(error: &KrometrailError) -> ExitCode {
    eprintln!(
        "error[{}] (retry={}): {}",
        error.code.as_str(),
        retry_advice_name(error.retry),
        error.message
    );
    if let Some(recovery) = &error.recovery {
        eprintln!("recovery: {recovery}");
    }
    ExitCode::from(FAILURE_EXIT_CODE)
}

fn retry_advice_name(advice: RetryAdvice) -> &'static str {
    match advice {
        RetryAdvice::Never => "never",
        RetryAdvice::Safe => "safe",
        RetryAdvice::AfterRecovery => "after_recovery",
    }
}
