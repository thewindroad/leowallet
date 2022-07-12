// Copyright (C) 2019-2022 Aleo Systems Inc.
// This file is part of the Leo library.

// The Leo library is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.

// The Leo library is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE. See the
// GNU General Public License for more details.

// You should have received a copy of the GNU General Public License
// along with the Leo library. If not, see <https://www.gnu.org/licenses/>.

use super::*;
use crate::{commands::Command, context::Context};
use leo_compiler::{Ast, Compiler, InputAst, OutputOptions};
use leo_errors::{CliError, Result};
use leo_package::{
    inputs::InputFile,
    // inputs::*,
    // outputs::CircuitFile
    outputs::{ChecksumFile, OutputsDirectory, MAIN_ALEO_FILE_NAME, OUTPUTS_DIRECTORY_NAME},
    source::{MainFile, MAIN_FILENAME, SOURCE_DIRECTORY_NAME},
};
use std::str::FromStr;

use clap::StructOpt;
use tracing::span::Span;

/// Compiler Options wrapper for Build command. Also used by other commands which
/// require Build command output as their input.
#[derive(StructOpt, Clone, Debug, Default)]
pub struct BuildOptions {
    #[structopt(long, help = "Enable spans in AST snapshots.")]
    pub enable_spans: bool,
    #[structopt(long, help = "Writes all AST snapshots for the different compiler phases.")]
    pub enable_all_ast_snapshots: bool,
    #[structopt(long, help = "Writes Input AST snapshot of the initial parse.")]
    pub enable_initial_input_ast_snapshot: bool,
    #[structopt(long, help = "Writes AST snapshot of the initial parse.")]
    pub enable_initial_ast_snapshot: bool,
    // Note: This is currently made optional since code generation is just a prototype.
    #[structopt(
        long,
        help = "Runs the code generation stage of the compiler and prints the resulting bytecode."
    )]
    pub enable_code_generation: bool,
}

impl From<BuildOptions> for OutputOptions {
    fn from(options: BuildOptions) -> Self {
        let mut out_options = Self {
            spans_enabled: options.enable_spans,
            input_ast_initial: options.enable_initial_input_ast_snapshot,
            ast_initial: options.enable_initial_ast_snapshot,
        };
        if options.enable_all_ast_snapshots {
            out_options.input_ast_initial = true;
            out_options.ast_initial = true;
        }

        out_options
    }
}

/// Compile and build program command.
#[derive(StructOpt, Debug)]
pub struct Build {
    #[structopt(flatten)]
    pub(crate) compiler_options: BuildOptions,
}

impl Command for Build {
    type Input = ();
    type Output = (Option<InputAst>, Ast, bool);

    fn log_span(&self) -> Span {
        tracing::span!(tracing::Level::INFO, "Build")
    }

    fn prelude(&self, _: Context) -> Result<Self::Input> {
        Ok(())
    }

    fn apply(self, context: Context, _: Self::Input) -> Result<Self::Output> {
        let path = context.dir()?;
        let manifest = context.manifest().map_err(|_| CliError::manifest_file_not_found())?;
        let package_name = manifest.program_id().name().to_string();
        // let imports_map = manifest.get_imports_map().unwrap_or_default();
        //
        // // Error out if there are dependencies but no lock file found.
        // if !imports_map.is_empty() && !context.lock_file_exists()? {
        //     return Err(CliError::dependencies_are_not_installed().into());
        // }

        // Sanitize the package path to the root directory.
        let mut package_path = path.clone();
        if package_path.is_file() {
            package_path.pop();
        }

        // Construct the path to the output directory.
        let mut output_directory = package_path.clone();
        output_directory.push(OUTPUTS_DIRECTORY_NAME);

        tracing::info!("Starting...");

        // Compile the main.leo file along with constraints
        if !MainFile::exists_at(&package_path) {
            return Err(CliError::package_main_file_not_found().into());
        }

        // Create the output directory
        OutputsDirectory::create(&package_path)?;

        // Construct the path to the main file in the source directory
        let mut main_file_path = package_path.clone();
        main_file_path.push(SOURCE_DIRECTORY_NAME);
        main_file_path.push(MAIN_FILENAME);

        // Load the input file at `package_name.in`
        let input_path = InputFile::new(&package_name).setup_file_path(&path);

        // Load the state file at `package_name.in`
        // let (state_string, state_path) = StateFile::new(&package_name).read_from(&path)?;

        // Log compilation of files to console
        tracing::info!("Compiling main program... ({:?})", main_file_path);

        // let imports_map = if context.lock_file_exists()? {
        //     context.lock_file()?.to_import_map()
        // } else {
        //     Default::default()
        // };

        // Load the program at `main_file_path`
        // let program = Compiler::<Fq, EdwardsGroupType>::parse_program_with_input(
        //     package_name.clone(),
        //     main_file_path,
        //     output_directory,
        //     &input_string,
        //     &input_path,
        //     &state_string,
        //     &state_path,
        //     thread_leaked_context(),
        //     Some(self.compiler_options.clone().into()),
        //     imports_map,
        //     Some(self.compiler_options.into()),
        // )?;

        // Initialize error handler
        let handler = leo_errors::emitter::Handler::default();

        // Create a new instance of the Leo compiler.
        let mut program = Compiler::new(
            package_name.to_string(),
            String::from("aleo"),
            &handler,
            main_file_path,
            output_directory,
            Some(self.compiler_options.into()),
        );
        program.parse_input(input_path.to_path_buf())?;

        // Compute the current program checksum
        let program_checksum = program.checksum()?;

        // Compile the program.
        {
            let program_id_string = format!("{}.aleo", package_name); // todo: read this from a config file.
            let (_, instructions) = program.compile_and_generate_instructions()?;

            // Parse the generated instructions into an Aleo file.
            let file =
                AleoFile::<Testnet3>::from_str(&instructions).expect("Failed to parse generated Aleo instructions.");

            // Write the Aleo file to `main.aleo`.
            let mut aleo_file_path = package_path.clone();
            aleo_file_path.push(MAIN_ALEO_FILE_NAME);

            file.write_to(&aleo_file_path).expect("Failed to write the aleo file.");

            // Initialize the program id.
            let program_id =
                ProgramID::<Testnet3>::from_str(&program_id_string).expect("Failed to parse program id from string.");

            // Create the program.json file to the output directory.
            let _manifest_file = if Manifest::<Testnet3>::exists_at(&package_path) {
                Manifest::<Testnet3>::open(&package_path).expect("Failed to open manifest file.")
            } else {
                Manifest::<Testnet3>::create(&package_path, &program_id).expect("Failed to create manifest file.")
            };

            // Call the `build` command from the Aleo SDK.
            // todo error: thread 'main' panicked at 'Failed to call `aleo build` command from the Aleo SDK: Development private key not found.', leo/commands/build.rs:203:37
            // let build = aleo::commands::Build;
            // let res = build.parse().expect("Failed to call `aleo build` command from the Aleo SDK");
            // tracing::info!("Result: {}", res);
        }

        // If a checksum file exists, check if it differs from the new checksum
        let checksum_file = ChecksumFile::new(&package_name);
        let checksum_differs = if checksum_file.exists_at(&package_path) {
            let previous_checksum = checksum_file.read_from(&package_path)?;
            program_checksum != previous_checksum
        } else {
            // By default, the checksum differs if there is no checksum to compare against
            true
        };

        // If checksum differs, compile the program
        if checksum_differs {
            // Write the new checksum to the output directory
            checksum_file.write_to(&path, program_checksum)?;

            tracing::debug!("Checksum saved ({:?})", path);
        }

        tracing::info!("Complete");

        Ok((program.input_ast, program.ast, checksum_differs))
    }
}
