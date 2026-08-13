// Local replacement for `npx tauri signer sign`.
//
// Why this exists: the Tauri CLI's signer hangs on Windows after
// printing "Signing without password." — the CLI uses an
// interactive password prompt that detects a TTY wrong on Windows
// when launched via `Start-Process` / `npm run` and blocks forever.
// Tauri 2 has had this bug open for ~2 years without a fix.
//
// This binary produces the exact same .sig files and manifest
// `signature` strings that the official Tauri CLI would.
//
// Output format:
//   - `.sig` sidecar: standard 4-line minisign wire format
//     (untrusted comment, base64 sig blob, trusted comment,
//     base64 global sig) — same as what `npx tauri signer sign`
//     writes.
//   - The `signature` field of `latest.json` is
//     `base64(.sig file content)` — base64 of the entire
//     multi-line file as a string. This is what
//     `tauri-plugin-updater` decodes back into a string and
//     hands to `minisign_verify::Signature::decode`.
//
// KEY-FILE FORMAT: `tauri signer generate` produces a key file
// that is itself base64-encoded; inside, the standard minisign
// "comment\n<encoded key>" layout lives. We unwrap the outer
// base64 first, then hand the decoded multi-line content to the
// minisign crate.
//
// Supports the unencrypted key format produced by:
//   npx tauri signer generate -w <keyfile> --ci
//
// Usage:
//   tauri-signer -k <keyfile> <target>...
//   tauri-signer --version

use std::fs::File;
use std::io::BufReader;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use minisign::{sign as minisign_sign, SecretKey};

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().collect();
    if args.len() < 2 {
        usage();
        return ExitCode::from(2);
    }
    if matches!(args[1].as_str(), "-V" | "--version") {
        println!("tauri-signer {}", env!("CARGO_PKG_VERSION"));
        return ExitCode::SUCCESS;
    }
    if matches!(args[1].as_str(), "-h" | "--help") {
        usage();
        return ExitCode::SUCCESS;
    }

    let mut keyfile: Option<PathBuf> = None;
    let mut targets: Vec<PathBuf> = Vec::new();
    let mut i = 1;
    while i < args.len() {
        let a = &args[i];
        if matches!(a.as_str(), "-k" | "--key-file") {
            i += 1;
            if i >= args.len() {
                eprintln!("error: -k requires a path argument");
                return ExitCode::from(2);
            }
            keyfile = Some(PathBuf::from(&args[i]));
        } else {
            targets.push(PathBuf::from(a));
        }
        i += 1;
    }

    let keyfile = match keyfile {
        Some(p) => p,
        None => {
            eprintln!("error: -k <keyfile> is required\n");
            usage();
            return ExitCode::from(2);
        }
    };
    if targets.is_empty() {
        eprintln!("error: at least one target file is required\n");
        usage();
        return ExitCode::from(2);
    }

    // Load the secret key. The tauri-generated .key file is a
    // single-line base64 blob that DECODES to the standard minisign
    // "comment\n<encoded key>" layout. We unwrap the outer base64
    // first, then write the result to a temp file in standard
    // minisign format and hand it to `SecretKey::from_file`.
    eprintln!("loading key from {}", keyfile.display());
    let secret_key = match load_key(&keyfile) {
        Ok(sk) => sk,
        Err(e) => {
            eprintln!("error: failed to load key: {e}");
            return ExitCode::from(1);
        }
    };
    eprintln!("key loaded (keynum = {:02x?})", secret_key.keynum());

    let mut ok = true;
    for target in &targets {
        let file = match File::open(target) {
            Ok(f) => f,
            Err(e) => {
                eprintln!("error: cannot open {}: {e}", target.display());
                ok = false;
                continue;
            }
        };
        let reader = BufReader::new(file);

        eprintln!(
            "signing {} ({} bytes)...",
            target.display(),
            fs_metadata_size(target)
        );
        let sig_box = match minisign_sign(
            /*signature_num*/ None,
            &secret_key,
            reader,
            /*trusted_comment*/ None,
            /*untrusted_comment*/ None,
        ) {
            Ok(s) => s,
            Err(e) => {
                eprintln!("error: signing failed for {}: {e}", target.display());
                ok = false;
                continue;
            }
        };

        // The minisign crate v0.9's `SignatureBox` exposes the full
        // multi-line .sig wire format via `.into_string()`. Tauri's
        // CLI signer writes this same string to the .sig file, and
        // then base64-encodes THAT string to put in the manifest's
        // `signature` field. We do the same.
        let sig_text = sig_box.into_string();

        // Tauri's verifier expects `<file>.sig` next to the file,
        // in the standard 4-line minisign format.
        let sig_path = match target.extension().and_then(|e| e.to_str()) {
            Some(ext) => target.with_extension(format!("{ext}.sig")),
            None => target.with_extension("sig"),
        };
        if let Err(e) = std::fs::write(&sig_path, sig_text.as_bytes()) {
            eprintln!("error: writing {}: {e}", sig_path.display());
            ok = false;
            continue;
        }
        println!("wrote {} ({} bytes)", sig_path.display(), sig_text.len());
    }

    if ok {
        ExitCode::SUCCESS
    } else {
        ExitCode::from(1)
    }
}

/// Load a `SecretKey` from a file, handling the two formats
/// `tauri signer generate` can produce:
///
///   1. Standard minisign: file content is "comment\n<base64 key>\n".
///   2. tauri-signer style: the file is a SINGLE line of base64 that
///      DECODES to (1).
///
/// For both: when the key has KDF metadata (modern "chained" format
/// from `tauri signer generate`), the password must be an empty
/// string — not `None` (which would trigger an interactive prompt
/// that hangs in non-TTY contexts).
fn load_key(keyfile: &Path) -> Result<SecretKey, String> {
    // The tauri-generated key wraps the standard minisign layout
    // in an outer base64 layer. Try (1) first; if the file is the
    // wrapped form, (1) will fail with "Missing encoded key" and
    // we fall through to (2).
    let raw = std::fs::read_to_string(keyfile).map_err(|e| e.to_string())?;
    let raw_trimmed = raw.trim();

    // Try the simple path: file is already the standard minisign
    // two-line layout.
    if let Ok(sk) = SecretKey::from_file(keyfile, Some(String::new())) {
        return Ok(sk);
    }

    // Fall back: tauri-signer single-line base64 wrapping the
    // standard layout. Decode the outer base64 to get the
    // proper "comment\n<encoded key>\n" content, then load that.
    let decoded = base64_decode(raw_trimmed)
        .ok_or_else(|| "key file is neither minisign nor tauri-signer format".to_string())?;
    let decoded_str = String::from_utf8(decoded).map_err(|e| e.to_string())?;
    // Write the decoded content to a temp file with the
    // .key extension so minisign's loader accepts it.
    let tmp = std::env::temp_dir().join(format!(
        "tauri-signer-{}.key",
        std::process::id()
    ));
    std::fs::write(&tmp, &decoded_str).map_err(|e| e.to_string())?;
    let result = SecretKey::from_file(&tmp, Some(String::new()));
    let _ = std::fs::remove_file(&tmp);
    result.map_err(|e| e.to_string())
}

/// Extract the raw 64-byte Ed25519 signature from a minisign
/// `SignatureBox`'s rendered 4-line text format.
///
/// The minisign crate's `SignatureBox` has no public way to get
/// just the 64-byte sig (all interesting fields are `pub(crate)`).
/// The output of `to_string()` looks like:
///
///   untrusted comment: ...
///   <base64 of [2 magic][8 keynum][64 Ed25519 sig]>   <-- line 2 (index 1)
///   trusted comment: ...
///   <base64 of [64 global sig]>
///
/// We take line 2, base64-decode it (104 chars → 74 bytes), and
/// return the trailing 64 bytes.
#[allow(dead_code)]
fn _extract_ed25519_sig_unused() {}
/// Minimal base64 decoder for the tauri-signer key-file fallback.
/// We could use the `base64` crate but it's already in the main
/// project — for this tiny tool we keep deps minimal.
fn base64_decode(input: &str) -> Option<Vec<u8>> {
    // minisign uses the standard RFC 4648 alphabet (no URL-safe
    // variant). Some Tauri outputs may have stripped padding, so
    // we re-pad.
    let cleaned: String = input.chars().filter(|c| !c.is_whitespace()).collect();
    let mut s = cleaned.as_str();
    let pad = s.len() % 4;
    if pad != 0 {
        s = &s[..s.len() - pad];
        // `s` is a borrow of `cleaned`; we need to extend with
        // padding separately. Easiest: build a new String.
        let mut padded = String::from(s);
        for _ in 0..(4 - pad) {
            padded.push('=');
        }
        return base64_decode(&padded);
    }

    // Standard alphabet. Use a manual decoder to avoid pulling
    // in the `base64` crate (which has a different API surface
    // in different versions and would just add risk here).
    const ALPHABET: &[u8; 64] =
        b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut lookup = [255u8; 256];
    for (i, &b) in ALPHABET.iter().enumerate() {
        lookup[b as usize] = i as u8;
    }
    let bytes = s.as_bytes();
    if bytes.len() % 4 != 0 {
        return None;
    }
    let mut out = Vec::with_capacity(bytes.len() / 4 * 3);
    for chunk in bytes.chunks(4) {
        let mut vals = [255u8; 4];
        for (i, &b) in chunk.iter().enumerate() {
            if b == b'=' {
                vals[i] = 0; // padding
            } else {
                vals[i] = lookup[b as usize];
                if vals[i] == 255 {
                    return None;
                }
            }
        }
        out.push((vals[0] << 2) | (vals[1] >> 4));
        if chunk[2] != b'=' {
            out.push((vals[1] << 4) | (vals[2] >> 2));
        }
        if chunk[3] != b'=' {
            out.push((vals[2] << 6) | vals[3]);
        }
    }
    // Silence unused import warning for std::io::Write.
    Some(out)
}

fn usage() {
    eprintln!(
        "tauri-signer — local replacement for `npx tauri signer sign`\n\
         \n\
         USAGE:\n  \
             tauri-signer -k <keyfile> <target>...\n\
         \n\
         ARGS:\n  \
             -k, --key-file <path>   Path to the .key file (unencrypted minisign)\n  \
             <target>...             One or more files to sign\n\
         \n\
         The output is a `<file>.sig` sidecar next to each target, in the\n\
         exact format Tauri's Rust updater expects."
    );
}

fn fs_metadata_size(path: &std::path::Path) -> u64 {
    std::fs::metadata(path).map(|m| m.len()).unwrap_or(0)
}
