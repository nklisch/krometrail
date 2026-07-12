mod app;
mod cli;

use std::process::ExitCode;

use clap::{CommandFactory, Parser};
use krometrail_core::{ErrorCode, KrometrailError, RetryAdvice};

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
        error_code_name(error.code),
        retry_advice_name(error.retry),
        error.message
    );
    if let Some(recovery) = &error.recovery {
        eprintln!("recovery: {recovery}");
    }
    ExitCode::from(FAILURE_EXIT_CODE)
}

fn error_code_name(code: ErrorCode) -> &'static str {
    match code {
        ErrorCode::InvalidInput => "invalid_input",
        ErrorCode::InvalidLifecycleTransition => "invalid_lifecycle_transition",
        ErrorCode::InvalidTime => "invalid_time",
        ErrorCode::NotFound => "not_found",
        ErrorCode::Unsupported => "unsupported",
        ErrorCode::BrowserDisconnected => "browser_disconnected",
        ErrorCode::CaptureRejected => "capture_rejected",
        ErrorCode::PersistenceFailed => "persistence_failed",
        ErrorCode::BudgetExhausted => "budget_exhausted",
        ErrorCode::Internal => "internal",
    }
}

fn retry_advice_name(advice: RetryAdvice) -> &'static str {
    match advice {
        RetryAdvice::Never => "never",
        RetryAdvice::Safe => "safe",
        RetryAdvice::AfterRecovery => "after_recovery",
    }
}
