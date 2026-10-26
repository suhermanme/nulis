# Installing Nulis

Download the file for your system from the [latest release](https://github.com/suhermanme/nulis/releases/latest).

## macOS

**With [Homebrew](https://brew.sh):**

```sh
brew tap suhermanme/nulis https://github.com/suhermanme/nulis
brew trust suhermanme/nulis   # let Homebrew load this non-official tap
brew install --cask nulis
```

`brew trust` is only needed on Homebrew versions that gate non-official taps; if your Homebrew does not have that command yet, skip the line. Without it you get *Refusing to load cask nulis from untrusted tap suhermanme/nulis*.

`brew upgrade --cask nulis` installs new releases, and `brew uninstall --cask nulis` removes Nulis (add `--zap` to also delete its settings and data). The first time you open Nulis, macOS blocks it; allow it once as described [below](#macos).

**If Homebrew says the latest version is already installed**, but Nulis is missing, outdated, or fails to upgrade, the tap is stale or an earlier install left broken files behind. Refresh the tap, clear the old install, and install again:

```sh
brew update                                    # third-party taps only refresh here
git -C "$(brew --repository)/Library/Taps/suhermanme/homebrew-nulis" pull --ff-only
brew uninstall --cask --force nulis
brew install --cask nulis
```

Homebrew fetches third-party taps only during `brew update`, so a `brew install` run right after a new release can still see the previous version. If an install was interrupted or the app was moved or deleted, `brew upgrade --cask nulis` stops with *It seems the App source '/Applications/Nulis.app' is not there*; the forced uninstall above clears that. Check the result with `brew info --cask nulis`.

**On an Intel Mac**, Homebrew may warn that it no longer builds bottles for `macOS on Intel x86_64` and that you are on a Tier 3 configuration. That warning is about compiled *formulae* and does not affect this cask: Nulis ships a single universal DMG containing both `x86_64` and `arm64`, so it runs on Intel directly:

```sh
lipo -archs /Applications/Nulis.app/Contents/MacOS/nulis-desktop
# x86_64 arm64
```

**Or by hand:**

1. Download `Nulis_<version>_universal.dmg`. It runs on both Apple silicon and Intel Macs.
2. Open the `.dmg` and drag **Nulis** into **Applications**.
3. Open Nulis from Applications.

**The first time, macOS blocks it.** You will see a message such as *"Nulis" Not Opened — Apple could not verify "Nulis" is free of malware*. This is expected: Nulis is not notarized by Apple and is not signed with a Developer ID certificate, both of which require a paid Apple Developer membership. The build carries only an ad-hoc signature: it seals the bundle, so macOS reports it as unverified and offers **Open Anyway** rather than calling the app damaged, but it names no developer and cannot prove the app was not altered. You allow it once:

1. In the message, click **Done** (do not choose Move to Trash).
2. Open **System Settings ▸ Privacy & Security**.
3. Scroll down to **Security**. Next to *"Nulis" was blocked to protect your Mac*, click **Open Anyway**.
4. Click **Open Anyway** again in the confirmation and enter your password or use Touch ID.

Nulis opens, and from then on it opens normally like any other app. You only do this once per installed version.

- The **Open Anyway** button appears only after you have tried to open Nulis, and it stays for about an hour. If you do not see it, open Nulis from Applications again and go back to Privacy & Security.
- On recent macOS versions, right-clicking the app and choosing **Open** no longer offers a way past this message; use **Open Anyway** in System Settings.

## Windows

Download `Nulis_<version>_x64-setup.exe` and run it. It installs for your user account without administrator rights. If Windows SmartScreen shows *Windows protected your PC*, click **More info**, then **Run anyway**: the installer is not signed with a paid code-signing certificate.

## Linux

Download the package for your distribution and install it with your package manager, which also installs the libraries Nulis needs:

```sh
# Debian, Ubuntu, and derivatives
sudo apt install ./Nulis_<version>_amd64.deb

# Fedora, RHEL, openSUSE, and derivatives
sudo dnf install ./Nulis-<version>-1.x86_64.rpm
```

Each release also has a `SHA256SUMS.txt` to check the downloads: `sha256sum -c SHA256SUMS.txt --ignore-missing` on Linux, or `shasum -a 256 -c SHA256SUMS.txt --ignore-missing` on macOS.
