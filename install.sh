#!/bin/sh
set -eu

# ──────────────────────────────────────────────────────────
# forged installer
# Usage: curl -fsSL https://raw.githubusercontent.com/AlphaLawless/forged/main/install.sh | sh
# ──────────────────────────────────────────────────────────

REPO="AlphaLawless/forged"
BINARY="forged"

main() {
    need_cmd curl
    need_cmd tar
    need_cmd uname

    os="$(detect_os)"
    arch="$(detect_arch)"

    if [ "$os" = "windows" ]; then
        err "Windows is not supported by this installer. Download the .zip from:"
        err "  https://github.com/${REPO}/releases/latest"
        exit 1
    fi

    asset="${BINARY}-${os}-${arch}.tar.gz"
    url="$(get_latest_url "$asset")"

    printf "  Installing %s (%s-%s)...\n" "$BINARY" "$os" "$arch"

    tmpdir="$(mktemp -d)"
    trap 'rm -rf "$tmpdir"' EXIT

    printf "  Downloading %s\n" "$url"
    curl -fsSL "$url" -o "${tmpdir}/${asset}"

    tar xzf "${tmpdir}/${asset}" -C "$tmpdir"

    install_dir="$(get_install_dir)"
    mkdir -p "$install_dir"
    mv "${tmpdir}/${BINARY}" "${install_dir}/${BINARY}"
    chmod +x "${install_dir}/${BINARY}"

    printf "  \033[32m✔\033[0m Installed to %s/%s\n" "$install_dir" "$BINARY"

    # Check if install dir is in PATH
    case ":$PATH:" in
        *":${install_dir}:"*) ;;
        *)
            printf "\n  \033[33m⚠\033[0m %s is not in your PATH.\n" "$install_dir"
            printf "  Add this to your shell profile:\n"
            printf "    export PATH=\"%s:\$PATH\"\n\n" "$install_dir"
            ;;
    esac

    if command -v "$BINARY" >/dev/null 2>&1; then
        version="$("$BINARY" --version 2>/dev/null || echo "unknown")"
        printf "  %s\n" "$version"
    fi
}

detect_os() {
    case "$(uname -s)" in
        Linux*)  echo "linux" ;;
        Darwin*) echo "darwin" ;;
        MINGW*|MSYS*|CYGWIN*) echo "windows" ;;
        *) err "Unsupported OS: $(uname -s)"; exit 1 ;;
    esac
}

detect_arch() {
    case "$(uname -m)" in
        x86_64|amd64)  echo "x86_64" ;;
        aarch64|arm64) echo "aarch64" ;;
        *) err "Unsupported architecture: $(uname -m)"; exit 1 ;;
    esac
}

get_latest_url() {
    asset="$1"
    # Use GitHub API to find the latest release download URL
    release_url="https://api.github.com/repos/${REPO}/releases/latest"
    url="$(curl -fsSL "$release_url" | grep "browser_download_url.*${asset}" | head -1 | cut -d '"' -f 4)"

    if [ -z "$url" ]; then
        err "Could not find release asset: ${asset}"
        err "Check https://github.com/${REPO}/releases"
        exit 1
    fi

    echo "$url"
}

get_install_dir() {
    if [ "$(id -u)" = "0" ]; then
        echo "/usr/local/bin"
    else
        echo "${HOME}/.local/bin"
    fi
}

need_cmd() {
    if ! command -v "$1" >/dev/null 2>&1; then
        err "Required command not found: $1"
        exit 1
    fi
}

err() {
    printf "  \033[31mError:\033[0m %s\n" "$1" >&2
}

main
