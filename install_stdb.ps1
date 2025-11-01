<#
.SYNOPSIS
    Installs SpacetimeDB and adds it to the user's PATH.

.DESCRIPTION
    Downloads the latest SpacetimeDB installer, runs it, and updates the user's PATH
    to include the installation directory if not already present.
#>

Param(
    [Parameter(Mandatory = $false)]
    [Switch]$Nightly
)

function Install {
    # Stop on all errors
    $ErrorActionPreference = 'Stop'

    # Ensure TLS 1.2 is used for secure downloads
    [Net.ServicePointManager]::SecurityProtocol = [Net.SecurityProtocolType]::Tls12

    # URL for the latest SpacetimeDB installer
    $DownloadUrl = "https://github.com/clockworklabs/SpacetimeDB/releases/latest/download/spacetimedb-update-x86_64-pc-windows-msvc.exe"

    Write-Output "Downloading installer..."

    # Adds a directory to the user's PATH if not already present
    function UpdatePathIfNotExists {
        param (
            [string]$DirectoryToAdd
        )
        $currentPath = [Environment]::GetEnvironmentVariable("Path", "User")
        if (-not $currentPath.Contains($DirectoryToAdd)) {
            [Environment]::SetEnvironmentVariable("Path", $currentPath + ";" + $DirectoryToAdd, "User")
        }
    }

    # Download the installer to a temporary location
    $Executable = Join-Path ([System.IO.Path]::GetTempPath()) "spacetime-install.exe"
    Invoke-WebRequest $DownloadUrl -OutFile $Executable -UseBasicParsing

    # Run the installer
    & $Executable

    # TODO: do this in spacetimedb-update
    # Determine the install directory
    $InstallDir = Join-Path ([Environment]::GetFolderPath("LocalApplicationData")) "SpacetimeDB"

    # Add install directory to PATH if needed
    UpdatePathIfNotExists $InstallDir

    Write-Output "We have added spacetimedb to your Path. You may have to logout and log back in to reload your environment."
}

# Run the install function
Install