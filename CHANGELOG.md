# Version 0.3.1 (2025/08/10)

Simple release to fix just two error messages and the `get_games` method.

- Modified the `get_games` method to allow to return the `GamesNotOwned` error.
- Fixed the wording of the `UnknownSteamworksError` error.

# Version 0.3.0 (2025/08/10)

Added new features woohoo.

- Added a command to get the current mod version with the supported game versions.
- Added a command to get the current game server info.
- Fixed a bug where the downloads where actually not being parallelized thanks to my stupidity lol. Now it's fixed and actually faster (who knew?).

You should always checkout the help of each command to see what they can do.

# Version 0.2.0 (2025/08/10)

Added the features that I didn't add in the MVP version and fixed a little bug.

- Added file download retrying. The retry count can be configured with the `--retry-count` flag.
- Added the `--no-verify` flag to skip the file verification step when using the `update` command or the `run` one.
- Added the `--no-retry` flag to skip the retrying of failed downloads when using the `update` command.
- Fixed a little bug where the verifying progress bar wasn't shown when the files were being reverified for the second time.

# Version 0.1.2 (2025/08/09)

Woops, seems like I forgot AGAIN to completely test the output files before releasing the first version.

- Fixed (again) a bug where the hardcoded content folder was being created in the directory where the executable is located (which may not have write permissions) instead of the AppData directory.

# Version 0.1.1 (2025/08/09)

Fixing bugs and adding a little thing because I forgotten to test it correctly before releasing the first version lol.

- Fixed a bug where the hardcoded content folder was being created in the directory where the executable was being run instead of the directory where the executable is located.
- Added a progress bar to indicate the progress (how obvious can that be?) of the current download progress.

# Unreleased

what this? beep beep boop :o
