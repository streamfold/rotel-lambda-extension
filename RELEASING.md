# Releasing

This document describes the release process for the Rotel Lambda Extension.

## Overview

The release process is fully automated using GitHub Actions workflows. You simply need to trigger a version bump, and the rest happens automatically:

1. **Bump Version** - Manually trigger a version bump (patch/minor/major)
2. **Auto Tag** - Tag is automatically created when the version bump PR is merged
3. **Auto Release** - GitHub release is automatically created when the tag is pushed
4. **Build & Deploy** - Lambda extension layers are built and published to all AWS regions
5. **Update Release Notes** - Release notes are updated with layer version information

## Quick Start

### Standard Release Process

1. **Go to GitHub Actions**
   - Navigate to: [Actions → Bump Version](../../actions/workflows/bump-version.yml)
   - Click "Run workflow"

2. **Select Version Bump Type**
   - **patch**: Bug fixes and minor changes (0.1.0 → 0.1.1)
   - **minor**: New features, backwards compatible (0.1.0 → 0.2.0)
   - **major**: Breaking changes (0.1.0 → 1.0.0)

3. **Review and Merge PR**
   - The workflow will create a PR with the version bump
   - Review the changes in `Cargo.toml` and `Cargo.lock`
   - Merge the PR when ready

4. **Automatic Release**
   - When the PR is merged, a tag (e.g., `v0.1.1`) is automatically created
   - A GitHub release is created with auto-generated release notes
   - Lambda extension layers are built and published to all regions
   - Release notes are updated with a table of layer versions by region

That's it! The entire process is automated after you merge the version bump PR.
