use crate::fs::*;
use crate::git::*;
use crate::ui::*;
use indicatif::ProgressBar;
use chrono::prelude::*;

use console::style;
use log::{debug, info};
use serde::{Deserialize, Serialize};

use thiserror::Error;

#[derive(Debug, Serialize, Deserialize, Eq, PartialEq)]
pub struct ConfigurationFile {
    pub project_name: String,
    pub branch: String,
    pub pinned_hash: String,
    pub files_to_copy: Vec<File>,
}

impl ConfigurationFile {
    pub fn is_empty(&self) -> bool {
        self == &ConfigurationFile::default()
    }

    fn needs_project_name(&self) -> bool {
        self.project_name == ""
    }

    fn needs_branch(&self) -> bool {
        self.branch == ""
    }

    fn needs_pinned_hash(&self) -> bool {
        self.pinned_hash == ""
    }
}

impl Default for ConfigurationFile {
    fn default() -> Self {
        let files_to_copy: Vec<File> = Vec::new();
        ConfigurationFile {
            project_name: "".to_string(),
            branch: "".to_string(),
            pinned_hash: "".to_string(),
            files_to_copy,
        }
    }
}

#[derive(Error, Debug)]
pub enum ConfigureError {

    #[error("Unable to initialize underlying encryption")]
    EncryptionUnavailable,

    #[error("Unable to decrypt file")]
    DataDecryptionError(#[from] std::io::Error),

    #[error("Invalid git status")]
    GitStatusParsingError(#[from] std::num::ParseIntError),

    #[error("Invalid git status")]
    GitStatusUnknownError,

    #[error("No secrets repository could be found on this machine")]
    SecretsNotPresent,

    #[error("An encrypted file is missing – unable to apply secrets to project. Run `configure update` to fix this")]
    EncryptedFileMissing,

    #[error("Unable to read keys.json file in your secrets repo")]
    KeysFileCannotBeRead,

    #[error("keys.json file in your secrets repo is not valid json")]
    KeysFileIsNotValidJSON,

    #[error("That project key is not defined in keys.json")]
    MissingProjectKey
}

#[derive(Debug, Serialize, Deserialize, Eq, PartialEq)]
pub struct File {
    #[serde(rename = "file")]
    pub source: String,
    pub destination: String,
}

impl File {
    pub fn get_encrypted_destination(&self) -> String {
        self.destination.clone() + &".enc".to_owned()
    }

    pub fn get_decrypted_destination(&self) -> String {
        self.destination.clone()
    }

    pub fn get_backup_destination(&self) -> String {
        let path = std::path::Path::new(&self.destination);

        let directory = match path.parent() {
            Some(parent) => parent,
            None => std::path::Path::new("/"),
        };


        let file_stem = path.file_stem().unwrap().to_str().unwrap_or("");
        let datetime = Local::now().format("%Y-%m-%d-%H-%M-%S").to_string();
        let extension = path.extension().unwrap_or(std::ffi::OsStr::new("")).to_str().unwrap_or("");

        let filename = format!("{:}-{:}.{:}.bak", file_stem, datetime, extension);

        return directory
            .join(filename)
            .to_str()
            .unwrap()
            .to_string();

    }
}

pub fn apply_configuration(configuration: ConfigurationFile) {
    // Decrypt the project's configuration files
    decrypt_files_for_configuration(&configuration).expect("Unable to decrypt and copy files");

    debug!("All Files Copied!");

    info!("Done")
}

pub fn update_configuration(mut configuration: ConfigurationFile) {
    let starting_branch =
        get_current_secrets_branch().expect("Unable to determine current secrets branch");
    let starting_ref =
        get_secrets_current_hash().expect("Unable to determine current secrets commit hash");

    heading("Configure Update");

    //
    // Step 1 – Fetch the latest secrets from the server
    //          We need them in order to update the pinned hash
    //
    let bar = ProgressBar::new_spinner();
    bar.enable_steady_tick(125);
    bar.set_message("Fetching Latest Secrets");

    fetch_secrets_latest_remote_data().expect("Unable to fetch latest secrets");

    bar.finish_and_clear();

    //
    // Step 2 – Check if the user wants to use a different secrets branch
    //
    configuration = prompt_for_branch(configuration, true);

    //
    // Step 3 – Check if the currente configuration branch is in sync with the server or not.or
    // If not, check with the user whether they'd like to continue
    //
    let status = get_secrets_repo_status().expect("Unable to get secrets repo status");

    let should_continue = match status.sync_state {
        RepoSyncState::Ahead => {
            warn(&format!(
                "Your local secrets repo has {:?} change(s) that the server does not",
                status.distance
            ));
            confirm("Would you like to continue?")
        }
        RepoSyncState::Behind => {
            warn(&format!(
                "The server has {:?} change(s) that your local secrets repo does not",
                status.distance
            ));
            confirm("Would you like to continue?")
        }
        RepoSyncState::Synced => true,
    };

    if !should_continue {
        return;
    }

    //
    // Step 4 – Check if the project's secrets are out of date compared to the server.
    //          If they out of date, we'll prompt the user to pull the latest remote
    //          changes into the local secrets repo before continuing.
    //
    let distance =
        configure_file_distance_behind_secrets_repo(&configuration, &configuration.branch);
    if distance > 0 {
        let message = format!(
            "This project is {:?} commit(s) behind the latest secrets. Would you like to use the latest secrets?",
            distance
        );

        // Prompt to update to most recent secrets data in the branch
        if confirm(&message) {
            let latest_commit_hash = get_latest_hash_for_remote_branch(&configuration.branch)
                .expect("Unable to fetch latest commit hash");

            debug!(
                "Moving the repo to {:?} at {:?}",
                &configuration.branch, latest_commit_hash
            );

            check_out_branch_at_revision(&configuration.branch, &latest_commit_hash)
                .expect("Unable to check out branch at revision");
            configuration.pinned_hash = latest_commit_hash;
        }
    }

    //
    // Step 5 – Write out encrypted files as needed
    //
    save_configuration(&configuration).expect("Unable to save updated configuration");

    //
    // Step 6 – Write out encrypted files as needed
    //
    write_encrypted_files_for_configuration(&configuration)
        .expect("Unable to copy encrypted files");

    //
    // Step 7 – Roll everything back to how it was before we started
    //
    crate::git::check_out_branch_at_revision(&starting_branch, &starting_ref)
        .expect("Unable to roll back to branch");

    //
    // Step 8 – Apply these changes to the current repo
    //
    apply_configuration(configuration);
}

pub fn validate_configuration(configuration: ConfigurationFile) {
    println!("{:?}", configuration);
}

pub fn setup_configuration(mut configuration: ConfigurationFile) {
    heading("Configure Setup");
    println!("Let's get configuration set up for this project.");
    newline();

    // Help the user set the `project_name` field
    configuration = prompt_for_project_name_if_needed(configuration);

    // Help the user set the `branch` field
    configuration = prompt_for_branch(configuration, true);

    // Set the latest automatically hash based on the selected branch
    configuration = set_latest_hash_if_needed(configuration);

    // Help the user add files
    configuration = prompt_to_add_files(configuration);

    info!("Writing changes to .configure");


    save_configuration(&configuration).expect("Unable to save configure file");

    // Create a key in `keys.json` for the project if one doesn't already exist
    if read_encryption_key(&configuration).unwrap() == None {
        generate_encryption_key(&configuration).expect("Unable to automatically generate an encryption key for this project");
    }
}

fn prompt_for_project_name_if_needed(mut configuration: ConfigurationFile) -> ConfigurationFile {
    // If there's already a project name, don't bother updating it
    if !configuration.needs_project_name() {
        return configuration;
    }

    let project_name = prompt("What is the name of your project?");
    configuration.project_name = project_name.clone();
    println!("Project Name set to: {:?}", project_name);

    configuration
}

fn prompt_for_branch(mut configuration: ConfigurationFile, force: bool) -> ConfigurationFile {
    // If there's already a branch set, don't bother updating it
    if !configuration.needs_branch() && !force {
        return configuration;
    }

    let secrets_repo_path = find_secrets_repo();
    let current_branch =
        get_current_secrets_branch().expect("Unable to determine current secrets branch");
    let branches = get_secrets_branches().expect("Unable to fetch secrets branches");

    println!(
        "We've found your secrets repository at {:?}",
        secrets_repo_path
    );
    newline();
    println!("Which branch would you like to use?");
    println!("Current Branch: {}", style(&current_branch).green());

    let selected_branch =
        select(branches, &current_branch).expect("Unable to read selected branch");

    configuration.branch = selected_branch.clone();
    println!("Secrets repo branch set to: {:?}", selected_branch);

    configuration
}

fn set_latest_hash_if_needed(mut configuration: ConfigurationFile) -> ConfigurationFile {
    if !configuration.needs_pinned_hash() {
        return configuration;
    }

    let latest_hash = get_secrets_latest_hash(&configuration.branch)
        .expect("Unable to fetch the latest secrets hash");
    configuration.pinned_hash = latest_hash;

    configuration
}

fn prompt_to_add_files(mut configuration: ConfigurationFile) -> ConfigurationFile {
    let mut files = configuration.files_to_copy;

    let mut message = "Would you like to add files?";

    if !files.is_empty() {
        message = "Would you like to add additional files?";
    }

    while confirm(message) {
        match prompt_to_add_file() {
            Some(file) => files.push(file),
            None => continue,
        }
    }

    configuration.files_to_copy = files;

    configuration
}

fn prompt_to_add_file() -> Option<File> {
    let relative_source_file_path =
        prompt("Enter the source file path (relative to the secrets root):");

    let secrets_root = match find_secrets_repo() {
        Ok(repo_path) => repo_path,
        Err(_) => return None,
    };

    let full_source_file_path = secrets_root.join(&relative_source_file_path);

    if !full_source_file_path.exists() {
        println!("Source File does not exist: {:?}", full_source_file_path);
        return None;
    }

    let relative_destination_file_path =
        prompt("Enter the destination file path (relative to the project root):");

    let project_root = find_project_root();
    let full_destination_file_path = project_root.join(&relative_destination_file_path);

    debug!("Destination: {:?}", full_destination_file_path);

    Some(File {
        source: relative_source_file_path,
        destination: relative_destination_file_path,
    })
}

fn configure_file_distance_behind_secrets_repo(
    configuration: &ConfigurationFile,
    branch_name: &str,
) -> i32 {
    debug!("Checking if configure file is behind secrets repo");

    let current_branch =
        get_current_secrets_branch().expect("Unable to get current secrets branch");
    debug!("Current branch is: {:?}", current_branch);

    let current_hash =
        get_secrets_current_hash().expect("Unable to get current secrets hash");
    debug!("Current hash is: {:?}", current_hash);

    check_out_branch(branch_name).expect("Unable to switch branches");

    let latest_hash = get_secrets_current_hash().unwrap();
    let distance = secrets_repo_distance_between(&configuration.pinned_hash, &latest_hash).unwrap();

    // Put things back how we found them
    crate::git::check_out_branch_at_revision(&current_branch, &current_hash)
        .expect("Unable to roll back to branch");

    distance
}
