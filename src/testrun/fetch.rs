//! What a server run needs from the machine: Java 25 or newer and the official server jar.
//!
//! The jar is downloaded from Mojang's own endpoints (with `curl`, which every supported system
//! has) once, checked against the SHA-1 Mojang lists for it, and cached. Downloading it means
//! accepting the Minecraft EULA, which `slab test` therefore asks for explicitly.

use std::path::{Path, PathBuf};
use std::process::Command;

const MANIFEST: &str = "https://piston-meta.mojang.com/mc/game/version_manifest_v2.json";
pub const EULA_URL: &str = "https://aka.ms/MinecraftEULA";

/// The first Java of at least version 25 among: the given path, `$JAVA`, `$JAVA_HOME`, `java`.
pub fn find_java(explicit: Option<&Path>) -> Result<PathBuf, String> {
    let mut candidates: Vec<PathBuf> = Vec::new();
    candidates.extend(explicit.map(Path::to_path_buf));
    candidates.extend(std::env::var_os("JAVA").map(PathBuf::from));
    candidates.extend(std::env::var_os("JAVA_HOME").map(|h| PathBuf::from(h).join("bin").join(if cfg!(windows) { "java.exe" } else { "java" })));
    candidates.push(PathBuf::from("java"));
    let mut seen = Vec::new();
    for c in candidates {
        let Ok(out) = Command::new(&c).arg("-version").output() else { continue };
        let text = String::from_utf8_lossy(&out.stderr).into_owned();
        match java_major(&text) {
            Some(n) if n >= 25 => return Ok(c),
            Some(n) => seen.push(format!("{} is Java {n}", c.display())),
            None => {}
        }
    }
    let found = if seen.is_empty() { "no Java was found".to_owned() } else { seen.join(", ") };
    Err(format!("Minecraft 26.x needs Java 25 or newer, and {found}. Install one (https://adoptium.net) and put it on your PATH, or pass `--java PATH`."))
}

/// The major version in the text of `java -version`: `openjdk version "25.0.1" ...` is 25.
fn java_major(text: &str) -> Option<u32> {
    let after = text.split("version \"").nth(1)?;
    after.split(|c: char| !c.is_ascii_digit()).next()?.parse().ok()
}

/// Where downloads are kept between runs.
pub fn cache_root() -> PathBuf {
    if let Some(d) = std::env::var_os("SLAB_CACHE_DIR") {
        return PathBuf::from(d);
    }
    let base = if cfg!(windows) {
        std::env::var_os("LOCALAPPDATA").map(PathBuf::from)
    } else if cfg!(target_os = "macos") {
        std::env::var_os("HOME").map(|h| PathBuf::from(h).join("Library").join("Caches"))
    } else {
        std::env::var_os("XDG_CACHE_HOME").map(PathBuf::from).or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".cache")))
    };
    base.unwrap_or_else(std::env::temp_dir).join("slab")
}

fn curl(args: &[&str]) -> Result<Vec<u8>, String> {
    let out = Command::new("curl").args(["-fsSL", "--retry", "2"]).args(args).output().map_err(|e| format!("could not run `curl` to download the server ({e}); install curl, or download the server jar yourself and pass `--jar PATH`"))?;
    if out.status.success() { Ok(out.stdout) } else { Err(format!("download failed: {}", String::from_utf8_lossy(&out.stderr).trim())) }
}

/// The server jar of a Minecraft version: `explicit`, `$SLAB_SERVER_JAR`, the cache, or a
/// download (only when the EULA is accepted).
pub fn server_jar(version: &str, explicit: Option<&Path>, eula_accepted: bool) -> Result<PathBuf, String> {
    if let Some(p) = explicit.map(Path::to_path_buf).or_else(|| std::env::var_os("SLAB_SERVER_JAR").map(PathBuf::from)) {
        return if p.is_file() { Ok(p) } else { Err(format!("the server jar {} does not exist", p.display())) };
    }
    let cached = cache_root().join("mc").join(format!("server-{version}.jar"));
    if cached.is_file() {
        return Ok(cached);
    }
    if !eula_accepted {
        return Err(format!(
            "`slab test` runs a real Minecraft {version} server, so it needs the official server jar. Downloading it from Mojang means you accept the Minecraft EULA ({EULA_URL}).\nRun again with `--accept-eula` (or set `accept-eula = true` under `[test]` in Slab.toml) to agree and download it once."
        ));
    }
    eprintln!("downloading the Minecraft {version} server from Mojang (once)...");
    let manifest = Json::parse(&String::from_utf8_lossy(&curl(&[MANIFEST])?))?;
    let entry = manifest.get("versions").and_then(Json::items).and_then(|vs| vs.iter().find(|v| v.get("id").and_then(Json::text) == Some(version))).ok_or_else(|| format!("Mojang lists no Minecraft version {version}"))?;
    let url = entry.get("url").and_then(Json::text).ok_or("the version manifest has no url")?;
    let details = Json::parse(&String::from_utf8_lossy(&curl(&[url])?))?;
    let server = details.get("downloads").and_then(|d| d.get("server")).ok_or_else(|| format!("Minecraft {version} has no server download"))?;
    let (jar_url, sha1) = (server.get("url").and_then(Json::text).ok_or("no server url")?, server.get("sha1").and_then(Json::text).ok_or("no server checksum")?);
    let bytes = curl(&[jar_url])?;
    if sha1_hex(&bytes) != sha1 {
        return Err("the downloaded server jar failed its SHA-1 check, so it was thrown away".to_owned());
    }
    std::fs::create_dir_all(cached.parent().expect("a parent")).map_err(|e| e.to_string())?;
    std::fs::write(&cached, bytes).map_err(|e| format!("cannot save the server jar: {e}"))?;
    Ok(cached)
}

pub fn sha1_hex(data: &[u8]) -> String {
    let mut h: [u32; 5] = [0x67452301, 0xEFCDAB89, 0x98BADCFE, 0x10325476, 0xC3D2E1F0];
    let mut msg = data.to_vec();
    msg.push(0x80);
    while msg.len() % 64 != 56 {
        msg.push(0);
    }
    msg.extend((data.len() as u64 * 8).to_be_bytes());
    for chunk in msg.chunks(64) {
        let mut w = [0u32; 80];
        for (i, word) in chunk.chunks(4).enumerate() {
            w[i] = u32::from_be_bytes([word[0], word[1], word[2], word[3]]);
        }
        for i in 16..80 {
            w[i] = (w[i - 3] ^ w[i - 8] ^ w[i - 14] ^ w[i - 16]).rotate_left(1);
        }
        let [mut a, mut b, mut c, mut d, mut e] = h;
        for (i, wi) in w.iter().enumerate() {
            let (f, k) = match i {
                0..=19 => ((b & c) | (!b & d), 0x5A827999),
                20..=39 => (b ^ c ^ d, 0x6ED9EBA1),
                40..=59 => ((b & c) | (b & d) | (c & d), 0x8F1BBCDC),
                _ => (b ^ c ^ d, 0xCA62C1D6u32),
            };
            let t = a.rotate_left(5).wrapping_add(f).wrapping_add(e).wrapping_add(k).wrapping_add(*wi);
            e = d;
            d = c;
            c = b.rotate_left(30);
            b = a;
            a = t;
        }
        for (x, y) in h.iter_mut().zip([a, b, c, d, e]) {
            *x = x.wrapping_add(y);
        }
    }
    h.iter().map(|x| format!("{x:08x}")).collect()
}

/// Just enough JSON to read Mojang's version manifests.
#[derive(Debug, PartialEq)]
pub enum Json {
    Null,
    Bool(bool),
    Num(f64),
    Str(String),
    Arr(Vec<Json>),
    Obj(Vec<(String, Json)>),
}

impl Json {
    pub fn parse(text: &str) -> Result<Json, String> {
        let mut p = JsonParser { s: text.as_bytes(), i: 0 };
        let v = p.value()?;
        p.ws();
        if p.i == p.s.len() { Ok(v) } else { Err(format!("unexpected text after the JSON value at byte {}", p.i)) }
    }

    pub fn get(&self, key: &str) -> Option<&Json> {
        match self {
            Json::Obj(fields) => fields.iter().find(|(k, _)| k == key).map(|(_, v)| v),
            _ => None,
        }
    }

    pub fn items(&self) -> Option<&[Json]> {
        if let Json::Arr(a) = self { Some(a) } else { None }
    }

    pub fn text(&self) -> Option<&str> {
        if let Json::Str(s) = self { Some(s) } else { None }
    }
}

struct JsonParser<'a> {
    s: &'a [u8],
    i: usize,
}

impl JsonParser<'_> {
    fn ws(&mut self) {
        while self.s.get(self.i).is_some_and(u8::is_ascii_whitespace) {
            self.i += 1;
        }
    }

    fn eat(&mut self, lit: &str) -> bool {
        if self.s[self.i..].starts_with(lit.as_bytes()) {
            self.i += lit.len();
            true
        } else {
            false
        }
    }

    fn value(&mut self) -> Result<Json, String> {
        self.ws();
        match self.s.get(self.i) {
            Some(b'{') => {
                self.i += 1;
                let mut fields = Vec::new();
                loop {
                    self.ws();
                    if self.eat("}") {
                        return Ok(Json::Obj(fields));
                    }
                    if !fields.is_empty() && !self.eat(",") {
                        return Err(format!("expected `,` at byte {}", self.i));
                    }
                    self.ws();
                    let key = self.string()?;
                    self.ws();
                    if !self.eat(":") {
                        return Err(format!("expected `:` at byte {}", self.i));
                    }
                    fields.push((key, self.value()?));
                }
            }
            Some(b'[') => {
                self.i += 1;
                let mut items = Vec::new();
                loop {
                    self.ws();
                    if self.eat("]") {
                        return Ok(Json::Arr(items));
                    }
                    if !items.is_empty() && !self.eat(",") {
                        return Err(format!("expected `,` at byte {}", self.i));
                    }
                    items.push(self.value()?);
                }
            }
            Some(b'"') => Ok(Json::Str(self.string()?)),
            Some(_) if self.eat("true") => Ok(Json::Bool(true)),
            Some(_) if self.eat("false") => Ok(Json::Bool(false)),
            Some(_) if self.eat("null") => Ok(Json::Null),
            Some(c) if c.is_ascii_digit() || *c == b'-' => {
                let start = self.i;
                while self.s.get(self.i).is_some_and(|c| c.is_ascii_digit() || b"+-.eE".contains(c)) {
                    self.i += 1;
                }
                std::str::from_utf8(&self.s[start..self.i]).ok().and_then(|t| t.parse().ok()).map(Json::Num).ok_or_else(|| format!("bad number at byte {start}"))
            }
            _ => Err(format!("unexpected character at byte {}", self.i)),
        }
    }

    fn string(&mut self) -> Result<String, String> {
        if !self.eat("\"") {
            return Err(format!("expected a string at byte {}", self.i));
        }
        let mut out = Vec::new();
        loop {
            let Some(&c) = self.s.get(self.i) else { return Err("unterminated string".to_owned()) };
            self.i += 1;
            match c {
                b'"' => return String::from_utf8(out).map_err(|e| e.to_string()),
                b'\\' => {
                    let Some(&e) = self.s.get(self.i) else { return Err("unterminated string".to_owned()) };
                    self.i += 1;
                    match e {
                        b'n' => out.push(b'\n'),
                        b't' => out.push(b'\t'),
                        b'r' => out.push(b'\r'),
                        b'u' => {
                            let hex = std::str::from_utf8(self.s.get(self.i..self.i + 4).ok_or("bad \\u escape")?).map_err(|e| e.to_string())?;
                            let ch = u32::from_str_radix(hex, 16).ok().and_then(char::from_u32).unwrap_or('\u{fffd}');
                            self.i += 4;
                            out.extend(ch.to_string().bytes());
                        }
                        other => out.push(other),
                    }
                }
                other => out.push(other),
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sha1_of_known_inputs() {
        assert_eq!(sha1_hex(b"abc"), "a9993e364706816aba3e25717850c26c9cd0d89d");
        assert_eq!(sha1_hex(b""), "da39a3ee5e6b4b0d3255bfef95601890afd80709");
        assert_eq!(sha1_hex(&[b'a'; 1000]), "291e9a6c66994949b57ba5e650361e98fc36b1ba");
    }

    #[test]
    fn json_is_read() {
        let j = Json::parse(r#"{"versions": [{"id": "26.3", "url": "https://x/aA", "n": -1.5e1}, {"id": "1"}], "ok": true, "none": null}"#).unwrap();
        let v = j.get("versions").and_then(Json::items).unwrap();
        assert_eq!(v[0].get("id").and_then(Json::text), Some("26.3"));
        assert_eq!(v[0].get("url").and_then(Json::text), Some("https://x/aA"));
        assert_eq!(v[0].get("n"), Some(&Json::Num(-15.0)));
        assert_eq!(j.get("ok"), Some(&Json::Bool(true)));
        assert!(Json::parse("{\"a\": }").is_err());
        assert!(Json::parse("[1] 2").is_err());
    }

    #[test]
    fn java_versions_are_read_from_the_banner() {
        assert_eq!(java_major("openjdk version \"25.0.1\" 2026-01-20\nOpenJDK Runtime"), Some(25));
        assert_eq!(java_major("java version \"1.8.0_292\""), Some(1));
        assert_eq!(java_major("openjdk version \"27\" 2027-03-16"), Some(27));
        assert_eq!(java_major("not java"), None);
    }
}
