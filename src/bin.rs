use log::{debug, LevelFilter};
use simplelog::CombinedLogger;
use simplelog::Config;
use simplelog::TermLogger;
use simplelog::TerminalMode;
use structopt::StructOpt;
use structopt_flags::GetWithDefault;

#[derive(StructOpt)]
#[structopt(
    name = "configure",
    about = "A command-line utility for applying configuration secrets with strong encryption"
)]
struct Options {
    #[structopt(subcommand)]
    command: Command,

    #[structopt(flatten)]
    verbose: structopt_flags::VerboseNoDef,
}

#[derive(StructOpt)]
enum Command {
    /// Update this project's encrypted secrets to the latest version
    ///
    /// This command will walk the user through updating a project's secrets by:
    /// 1. Ensuring that the secrets repository has all the latest data from the server
    /// 2. Checking if the user wants to change which secrets branch being used to fetch secrets
    /// 3. Prompting the user to update to the latest secrets
    /// 4.

    //switch the secrets repo to the pinned commit hash
    /// in the `.configure` file, then copy the files specified in the `files_to_copy` hash
    /// to their specified destination, encrypting them with the format $filename+".enc".

    /// This command will download the latest secrets commits from the repo
    /// and update the pinned commit hash in the `.configure` file to the newest commit
    /// in the branch specified by `.configure`.
    Update,

    /// Decrypt the current secrets for this project.
    ///
    Apply,

    /// Change secrets settings
    ///
    /// This command will provide step-by-step help to make changes to the secrets configuration.
    Init,

    /// Ensure the `.configure` file is valid
    Validate,

    /// Create a new encryption key for use with a project
    CreateKey,
}

pub fn main() {
    let options = Options::from_args();

    CombinedLogger::init(vec![TermLogger::new(
        options.verbose.get_with_default(LevelFilter::Info),
        Config::default(),
        TerminalMode::Mixed,
    )
    .unwrap()])
    .unwrap();

    debug!("libconfigure initialized");

    match Options::from_args().command {
        Command::Apply => configure::apply(),
        Command::Update => configure::update(),
        Command::Init => configure::init(),
        Command::Validate => configure::validate(),
        Command::CreateKey => println!("{:?}", configure::generate_encryption_key()),
    }
}
