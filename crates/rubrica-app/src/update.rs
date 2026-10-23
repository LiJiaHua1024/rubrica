//! The release channel a build measures itself against, and the version it carries.
//!
//! There is no network client here and there is not going to be one: the reader's whole
//! premise is a single executable that talks to nothing, and an HTTP stack would be a
//! dependency the process pays for on every page to answer a question asked once a
//! month. So a check is a *file* read. A channel file -- `update-channel.json`, beside
//! the executable unless `--update-source` names another one -- says which version the
//! channel carries, what changed and where the release page is; this module reads it,
//! compares it with [`VERSION`], and hands the answer to whoever asked.
//!
//! What that buys and what it does not is worth stating where the code is, because the
//! difference is the design rather than a shortcoming of it: the check can tell a reader
//! that the copy they are running is older than the release the file describes, and it can
//! show that release's notes before anything is downloaded. It cannot *discover* a release
//! by itself, because discovering one means asking a server. Fetching one is the browser's
//! job: the reader hands the release page's address to the shell and the download happens
//! where downloads already happen.
//!
//! The JSON reader at the bottom is deliberately a fraction of JSON -- an object, strings,
//! arrays of strings, numbers kept as the text they were written as -- and it is written
//! here rather than pulled in, for the same reason the network client is not here at all.
//! Anything it does not understand is `None` rather than a panic: a channel file is a file
//! a reader can edit, and a hand-edited file is allowed to be wrong.

use std::cmp::Ordering;
use std::fmt;
use std::path::{Path, PathBuf};

/// The name of the file a check reads, beside the executable unless told otherwise.
pub(crate) const CHANNEL_FILE: &str = "update-channel.json";

/// Where the source lives: what a page that is not on disk falls back to.
pub(crate) const REPOSITORY: &str = "https://github.com/LiJiaHua1024/rubrica";

/// Where a download happens: the release page, opened in whatever browser the shell picks.
pub(crate) const RELEASES_URL: &str = "https://github.com/LiJiaHua1024/rubrica/releases";

/// Where a reader writes down what the reader did wrong.
pub(crate) const ISSUES_URL: &str = "https://github.com/LiJiaHua1024/rubrica/issues/new";

/// The version this build carries.
///
/// Read from the crate's own manifest, which inherits the workspace's `version`, so the
/// number a reader is shown and the number CI tags a release with are one number in one
/// place. Nothing else in the program spells a version.
pub(crate) const VERSION: &str = env!("CARGO_PKG_VERSION");

/// A version, read as far as it matters for saying "this one is newer".
///
/// One to three dot-separated numbers with an optional pre-release tail, which is the
/// whole of what a release tag looks like here: `1.2.3`, `1.2`, `1.2.3-rc.1`. Comparison
/// is semver's precedence -- `1.2.3-rc.1` sorts *below* `1.2.3`, `rc.2` above `rc.1`,
/// numeric identifiers by value and below word ones -- so a reader on a release candidate
/// is told the final release of the same version is newer, which is exactly the case the
/// Labs channel exists for. Build metadata (`+ci.4`) carries no precedence and is dropped,
/// and a leading `v` is accepted so a tag can be pasted in as it is spelled.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Version {
    major: u64,
    minor: u64,
    patch: u64,
    pre: Vec<Identifier>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
enum Identifier {
    Number(u64),
    Word(String),
}

impl Version {
    /// Read a version, or answer `None` for anything that is not one.
    ///
    /// Every failure is a `None` rather than an error with a position: the only caller
    /// shows the reader the whole file's complaint, and a version that cannot be read is
    /// one unreadable field rather than a diagnosis worth spelling out.
    pub(crate) fn parse(text: &str) -> Option<Version> {
        let trimmed = text.trim();
        let spelled = trimmed
            .strip_prefix('v')
            .or_else(|| trimmed.strip_prefix('V'))
            .unwrap_or(trimmed);
        // The build metadata stands after a `+`: it carries no precedence, so it is read
        // only far enough to be sure it is metadata rather than a typo, and then dropped.
        let mut pieces = spelled.splitn(2, '+');
        let precedence = pieces.next().unwrap_or(spelled);
        if let Some(metadata) = pieces.next() {
            if !metadata.split('.').all(identifier_spelling) {
                return None;
            }
        }
        let (numbers, tail) = match precedence.split_once('-') {
            Some((numbers, tail)) => (numbers, Some(tail)),
            None => (precedence, None),
        };
        let mut parts = numbers.split('.');
        let major = number(parts.next()?)?;
        let minor = match parts.next() {
            Some(text) => number(text)?,
            None => 0,
        };
        let patch = match parts.next() {
            Some(text) => number(text)?,
            None => 0,
        };
        // A fourth number is a version this program has no opinion about, and inventing
        // one would make `1.2.3.4` compare equal to `1.2.3`.
        if parts.next().is_some() {
            return None;
        }
        let pre = match tail {
            None => Vec::new(),
            Some(tail) => {
                let mut identifiers = Vec::new();
                for identifier in tail.split('.') {
                    if !identifier_spelling(identifier) {
                        return None;
                    }
                    identifiers.push(match number(identifier) {
                        Some(value) => Identifier::Number(value),
                        None => Identifier::Word(identifier.to_string()),
                    });
                }
                identifiers
            }
        };
        Some(Version { major, minor, patch, pre })
    }

    /// Whether this is a release candidate rather than a release: the one thing the
    /// channel switch turns on.
    pub(crate) fn is_prerelease(&self) -> bool {
        !self.pre.is_empty()
    }

    /// The precedence between two versions, which is semver's own rule.
    fn precedence(&self, other: &Version) -> Ordering {
        self.major
            .cmp(&other.major)
            .then_with(|| self.minor.cmp(&other.minor))
            .then_with(|| self.patch.cmp(&other.patch))
            .then_with(|| match (self.pre.is_empty(), other.pre.is_empty()) {
                (true, true) => Ordering::Equal,
                // A version with a pre-release tail stands below the release it leads to.
                (true, false) => Ordering::Greater,
                (false, true) => Ordering::Less,
                (false, false) => identifiers_first(&self.pre, &other.pre),
            })
    }
}

impl Ord for Version {
    fn cmp(&self, other: &Version) -> Ordering {
        self.precedence(other)
    }
}

impl PartialOrd for Version {
    fn partial_cmp(&self, other: &Version) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl fmt::Display for Version {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}.{}.{}", self.major, self.minor, self.patch)?;
        for (i, identifier) in self.pre.iter().enumerate() {
            f.write_str(if i == 0 { "-" } else { "." })?;
            match identifier {
                Identifier::Number(value) => write!(f, "{value}")?,
                Identifier::Word(word) => f.write_str(word)?,
            }
        }
        Ok(())
    }
}

/// The first place two pre-release tails differ.
///
/// A numeric identifier is lower than a word one and compares by value, words compare as
/// text, and a tail that runs out first is the lower of the two.
fn identifiers_first(left: &[Identifier], right: &[Identifier]) -> Ordering {
    for (a, b) in left.iter().zip(right.iter()) {
        let order = match (a, b) {
            (Identifier::Number(x), Identifier::Number(y)) => x.cmp(y),
            (Identifier::Word(x), Identifier::Word(y)) => x.cmp(y),
            (Identifier::Number(_), Identifier::Word(_)) => Ordering::Less,
            (Identifier::Word(_), Identifier::Number(_)) => Ordering::Greater,
        };
        if order != Ordering::Equal {
            return order;
        }
    }
    left.len().cmp(&right.len())
}

/// A number written as digits, or nothing: `+1`, `-1`, `1e3`, a leading space and an
/// empty field are all not the number this wants, and `u64`'s own overflow is a `None`
/// rather than a wrap.
fn number(text: &str) -> Option<u64> {
    if text.is_empty() || !text.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    text.parse().ok()
}

/// Whether a dot-separated field of a version's half-written half is one: letters, digits
/// and hyphens, and at least one of them. A trailing `+` or `-` is a version somebody
/// stopped typing, and reading it as the version without the tail would compare a typo as
/// if it were the release it was aiming at.
fn identifier_spelling(text: &str) -> bool {
    !text.is_empty() && text.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-')
}

/// Which releases a reader is willing to be told about.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Channel {
    Stable,
    Labs,
}

impl Channel {
    pub(crate) fn parse(text: &str) -> Option<Channel> {
        match text.trim() {
            "stable" => Some(Channel::Stable),
            "labs" => Some(Channel::Labs),
            _ => None,
        }
    }

    pub(crate) fn name(self) -> &'static str {
        match self {
            Channel::Stable => "stable",
            Channel::Labs => "labs",
        }
    }

    /// Whether a release candidate is offered on this channel.
    pub(crate) fn allows_prerelease(self) -> bool {
        self == Channel::Labs
    }
}

/// What a channel file says.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Release {
    pub(crate) version: Version,
    /// The channel the file itself belongs to, when it says: informative, since which
    /// releases a *reader* accepts is their own switch and not the file's.
    pub(crate) channel: Option<Channel>,
    pub(crate) notes: Vec<String>,
    pub(crate) url: Option<String>,
}

/// What comparing this build with a channel's release came to.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum Outcome {
    /// The build is the channel's release, or newer than it.
    UpToDate,
    Newer {
        version: Version,
        notes: Vec<String>,
        url: Option<String>,
    },
    /// Newer, but a release candidate and the reader is on the stable channel.
    Skipped { version: Version },
}

/// Compare a build's version with what the channel carries.
///
/// The switch is about pre-releases and about nothing else: a Labs release candidate is
/// skipped on the stable channel, and a release is offered on both, because a reader who
/// switched to Labs did not ask to stop being told about releases.
pub(crate) fn check(current: &Version, release: &Release, allow_prerelease: bool) -> Outcome {
    if release.version <= *current {
        return Outcome::UpToDate;
    }
    if release.version.is_prerelease() && !allow_prerelease {
        return Outcome::Skipped { version: release.version.clone() };
    }
    Outcome::Newer {
        version: release.version.clone(),
        notes: release.notes.clone(),
        url: release.url.clone(),
    }
}

/// A channel file, read. The largest file this will even look at, so that pointing it at
/// the wrong file -- a video, a disk image -- is a sentence rather than a wait.
const MAX_CHANNEL_BYTES: u64 = 256 * 1024;

/// Read a channel file into the release it describes.
pub(crate) fn read_release(source: &Path) -> Result<Release, String> {
    let metadata = std::fs::metadata(source)
        .map_err(|error| format!("cannot read {}: {error}", source.display()))?;
    if !metadata.is_file() {
        return Err(format!("{} is not a file", source.display()));
    }
    if metadata.len() > MAX_CHANNEL_BYTES {
        return Err(format!(
            "{} is larger than a release channel ({MAX_CHANNEL_BYTES} bytes at most)",
            source.display()
        ));
    }
    let text = std::fs::read_to_string(source)
        .map_err(|error| format!("cannot read {}: {error}", source.display()))?;
    parse_release(&text).ok_or_else(|| {
        format!(
            "{} is not a release channel: it wants an object with `version`, and `notes` as an array of strings",
            source.display()
        )
    })
}

/// Read a release out of the text of a channel file.
pub(crate) fn parse_release(text: &str) -> Option<Release> {
    // A file written by an editor that marks its encoding starts with a byte order mark,
    // which is not part of the document: refusing it would be refusing a file that is
    // exactly the JSON it looks like.
    let text = text.strip_prefix('\u{feff}').unwrap_or(text);
    let document = json::document(text)?;
    let version = document.field("version")?.text().and_then(Version::parse)?;
    let channel = document.field("channel").and_then(json::Value::text).and_then(Channel::parse);
    let notes = match document.field("notes") {
        Some(value) => value.strings()?,
        None => Vec::new(),
    };
    let url = document
        .field("url")
        .and_then(json::Value::text)
        .filter(|url| openable_address(url))
        .map(str::to_string);
    Some(Release { version, channel, notes, url })
}

/// Whether an address is one the shell can be handed.
///
/// The same two schemes the reader's own links are allowed -- a channel file can be
/// written by anyone, and `file:` or a program's own scheme would turn "check for
/// updates" into "run this".
fn openable_address(url: &str) -> bool {
    let trimmed = url.trim();
    let lower = trimmed.to_ascii_lowercase();
    (lower.starts_with("https://") || lower.starts_with("http://")) && trimmed.len() > "https://".len()
}

/// The file a check reads when nothing names another: `update-channel.json` beside the
/// running executable, which is where a released archive puts it.
pub(crate) fn default_source() -> Option<PathBuf> {
    let exe = std::env::current_exe().ok()?;
    Some(exe.parent()?.join(CHANNEL_FILE))
}

/// What a headless check prints: the two versions, whether the channel's release is
/// newer, the notes a reader would be shown before downloading, and where it was read.
///
/// The lines are the report of one file read. They are the same facts the window's own
/// check puts in front of a reader, so what the menu does can be checked without a window.
pub(crate) fn inspect(source: &Path, channel: Channel) -> Result<Vec<String>, String> {
    let release = read_release(source)?;
    let current = Version::parse(VERSION)
        .ok_or_else(|| format!("this build's own version `{VERSION}` is not a version"))?;
    let outcome = check(&current, &release, channel.allows_prerelease());
    let mut lines = Vec::with_capacity(release.notes.len() + 8);
    lines.push(format!("current      : {current}"));
    lines.push(format!("latest       : {}", release.version));
    lines.push(format!(
        "has newer    : {}",
        if matches!(outcome, Outcome::Newer { .. }) { "yes" } else { "no" }
    ));
    lines.push(format!(
        "channel      : {} ({})",
        channel.name(),
        if channel.allows_prerelease() { "pre-releases accepted" } else { "pre-releases skipped" }
    ));
    lines.push(format!("source       : {}", source.display()));
    if let Some(from_file) = release.channel {
        lines.push(format!("file channel : {}", from_file.name()));
    }
    if let Outcome::Skipped { version } = &outcome {
        lines.push(format!("skipped      : {version} is a pre-release, and this channel skips those"));
    }
    for note in &release.notes {
        lines.push(format!("notes        : {note}"));
    }
    if let Some(url) = &release.url {
        lines.push(format!("url          : {url}"));
    }
    Ok(lines)
}

/// A fraction of JSON, written here because a channel file is four fields and a parser
/// for them is smaller than the dependency that would read them.
mod json {
    /// The deepest nesting this parser will follow. A channel file is two levels deep;
    /// the limit is here so that a file full of brackets is refused rather than recursed
    /// into until the stack runs out.
    const MAX_DEPTH: usize = 16;

    #[derive(Clone, Debug, PartialEq)]
    pub(super) enum Value {
        Object(Vec<(String, Value)>),
        Array(Vec<Value>),
        Text(String),
        /// A number, kept as the literal that was written: nothing reads one, but a file
        /// carrying one is still a file this can walk past.
        Number(String),
        Bool(bool),
        Null,
    }

    impl Value {
        pub(super) fn text(&self) -> Option<&str> {
            match self {
                Value::Text(text) => Some(text),
                _ => None,
            }
        }

        /// One field of an object, by name. A field that is not there, and a value that is
        /// not an object at all, both answer `None`.
        pub(super) fn field(&self, name: &str) -> Option<&Value> {
            match self {
                Value::Object(fields) => {
                    fields.iter().find(|(key, _)| key == name).map(|(_, value)| value)
                }
                _ => None,
            }
        }

        /// The value as an array of strings, or `None` if it is anything else -- an
        /// array with a number in it is not a list of notes.
        pub(super) fn strings(&self) -> Option<Vec<String>> {
            match self {
                Value::Array(items) => items.iter().map(Value::text).map(|text| text.map(str::to_string)).collect(),
                _ => None,
            }
        }
    }

    /// Read one JSON document. Whitespace around it is allowed; anything after it is not,
    /// since two documents in one file is a file somebody concatenated by accident.
    pub(super) fn document(text: &str) -> Option<Value> {
        let mut parser = Parser { chars: text.chars().collect(), at: 0, depth: 0 };
        parser.skip_space();
        let value = parser.value()?;
        parser.skip_space();
        if parser.peek().is_some() {
            return None;
        }
        Some(value)
    }

    struct Parser {
        chars: Vec<char>,
        at: usize,
        depth: usize,
    }

    impl Parser {
        fn peek(&self) -> Option<char> {
            self.chars.get(self.at).copied()
        }

        fn bump(&mut self) -> Option<char> {
            let found = self.peek();
            if found.is_some() {
                self.at += 1;
            }
            found
        }

        fn skip_space(&mut self) {
            while matches!(self.peek(), Some(c) if c.is_whitespace()) {
                self.at += 1;
            }
        }

        fn eat(&mut self, want: char) -> bool {
            if self.peek() == Some(want) {
                self.at += 1;
                true
            } else {
                false
            }
        }

        fn literal(&mut self, word: &str) -> bool {
            let end = self.at + word.chars().count();
            if end > self.chars.len() {
                return false;
            }
            if self.chars[self.at..end].iter().copied().eq(word.chars()) {
                self.at = end;
                true
            } else {
                false
            }
        }

        fn value(&mut self) -> Option<Value> {
            if self.depth >= MAX_DEPTH {
                return None;
            }
            self.skip_space();
            match self.peek()? {
                '{' => self.object(),
                '[' => self.array(),
                '"' => self.string().map(Value::Text),
                't' => self.literal("true").then_some(Value::Bool(true)),
                'f' => self.literal("false").then_some(Value::Bool(false)),
                'n' => self.literal("null").then_some(Value::Null),
                c if c == '-' || c.is_ascii_digit() => self.number(),
                _ => None,
            }
        }

        fn object(&mut self) -> Option<Value> {
            self.depth += 1;
            let mut fields = Vec::new();
            if !self.eat('{') {
                return None;
            }
            self.skip_space();
            if self.eat('}') {
                self.depth -= 1;
                return Some(Value::Object(fields));
            }
            loop {
                self.skip_space();
                let key = self.string()?;
                self.skip_space();
                if !self.eat(':') {
                    return None;
                }
                let value = self.value()?;
                fields.push((key, value));
                self.skip_space();
                if self.eat(',') {
                    continue;
                }
                if self.eat('}') {
                    break;
                }
                return None;
            }
            self.depth -= 1;
            Some(Value::Object(fields))
        }

        fn array(&mut self) -> Option<Value> {
            self.depth += 1;
            let mut items = Vec::new();
            if !self.eat('[') {
                return None;
            }
            self.skip_space();
            if self.eat(']') {
                self.depth -= 1;
                return Some(Value::Array(items));
            }
            loop {
                items.push(self.value()?);
                self.skip_space();
                if self.eat(',') {
                    continue;
                }
                if self.eat(']') {
                    break;
                }
                return None;
            }
            self.depth -= 1;
            Some(Value::Array(items))
        }

        fn number(&mut self) -> Option<Value> {
            let start = self.at;
            while matches!(self.peek(), Some(c) if c.is_ascii_digit() || matches!(c, '-' | '+' | '.' | 'e' | 'E')) {
                self.at += 1;
            }
            if self.at == start {
                return None;
            }
            Some(Value::Number(self.chars[start..self.at].iter().collect()))
        }

        fn string(&mut self) -> Option<String> {
            if !self.eat('"') {
                return None;
            }
            let mut out = String::new();
            loop {
                match self.bump()? {
                    '"' => return Some(out),
                    '\\' => match self.bump()? {
                        '"' => out.push('"'),
                        '\\' => out.push('\\'),
                        '/' => out.push('/'),
                        'b' => out.push('\u{8}'),
                        'f' => out.push('\u{c}'),
                        'n' => out.push('\n'),
                        'r' => out.push('\r'),
                        't' => out.push('\t'),
                        'u' => out.push(self.codepoint()?),
                        _ => return None,
                    },
                    // A raw control character is not allowed inside a JSON string, and
                    // allowing one here would let a note carry a line the writer did not.
                    c if (c as u32) < 0x20 => return None,
                    c => out.push(c),
                }
            }
        }

        fn hex4(&mut self) -> Option<u32> {
            let mut value = 0u32;
            for _ in 0..4 {
                value = value * 16 + self.bump()?.to_digit(16)?;
            }
            Some(value)
        }

        /// One `\u` escape. A pair of escapes is one character and a lone surrogate is
        /// not a character, so it becomes the replacement mark rather than a failure:
        /// the file is still a channel file, one of its notes just has a hole in it.
        fn codepoint(&mut self) -> Option<char> {
            let first = self.hex4()?;
            if (0xD800..=0xDBFF).contains(&first) {
                if !self.eat('\\') || !self.eat('u') {
                    return Some('\u{FFFD}');
                }
                let second = self.hex4()?;
                if !(0xDC00..=0xDFFF).contains(&second) {
                    return Some('\u{FFFD}');
                }
                let joined = 0x10000 + ((first - 0xD800) << 10) + (second - 0xDC00);
                return char::from_u32(joined).or(Some('\u{FFFD}'));
            }
            if (0xDC00..=0xDFFF).contains(&first) {
                return Some('\u{FFFD}');
            }
            char::from_u32(first).or(Some('\u{FFFD}'))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn version(text: &str) -> Version {
        Version::parse(text).unwrap_or_else(|| panic!("{text} is a version"))
    }

    /// The comparisons a reader would actually ask about, written as the answer rather
    /// than as an ordering of the two parsings: `Less` is "the right one is newer".
    #[test]
    fn the_version_pairs_a_reader_would_ask_about() {
        let cases: &[(&str, &str, Ordering)] = &[
            ("1.2.3", "1.2.3", Ordering::Equal),
            ("1.2.3", "1.2.4", Ordering::Less),
            ("1.2.4", "1.2.3", Ordering::Greater),
            ("1.2.3", "1.3.0", Ordering::Less),
            ("1.2.3", "2.0.0", Ordering::Less),
            ("0.9.1", "0.9.2", Ordering::Less),
            ("0.9.1", "0.10.0", Ordering::Less),
            ("0.9.1", "0.9.10", Ordering::Less),
            // A release candidate leads to its release and stands below it.
            ("1.2.3-rc.1", "1.2.3", Ordering::Less),
            ("1.2.3", "1.2.3-rc.1", Ordering::Greater),
            ("1.2.3-rc.1", "1.2.3-rc.2", Ordering::Less),
            ("1.2.3-rc.2", "1.2.3-rc.10", Ordering::Less),
            ("1.2.3-rc.1", "1.2.4-rc.1", Ordering::Less),
            ("1.2.3-alpha", "1.2.3-beta", Ordering::Less),
            ("1.2.3-alpha.1", "1.2.3-alpha", Ordering::Greater),
            // A numeric identifier is below a word one, by semver's own rule.
            ("1.2.3-1", "1.2.3-alpha", Ordering::Less),
            ("1.2.3-alpha.1", "1.2.3-alpha.beta", Ordering::Less),
            // Two and three numbers are the same version written two ways.
            ("1.2", "1.2.0", Ordering::Equal),
            ("1", "1.0.0", Ordering::Equal),
            ("1.2", "1.2.1", Ordering::Less),
            // A tag pasted in as the release page spells it, and metadata with no
            // precedence of its own.
            ("v1.2.3", "1.2.3", Ordering::Equal),
            ("1.2.3+ci.4", "1.2.3", Ordering::Equal),
            ("  1.2.3  ", "1.2.3", Ordering::Equal),
        ];
        for (left, right, expected) in cases {
            let left = version(left);
            let right = version(right);
            assert_eq!(left.cmp(&right), *expected, "{left} against {right}");
        }
    }

    #[test]
    fn a_version_that_is_not_one_is_refused() {
        for text in [
            "", "   ", "abc", "v", "1.2.x", "1.2.3.4", "1..2", "1.2.", ".1.2", "1.2.3-",
            "-rc.1", "1.2.3-rc..1", "1.2.3-alpha+", "1.2.3+", "1.2.3+ci..4", "1.2.3-rc.1+",
            "1.2.3-rc_1", "one.two.three", "1,2,3", "1.2.3\n4", "١.٢.٣",
        ] {
            assert!(Version::parse(text).is_none(), "{text:?} is not a version");
        }
        // And a number too large for the field it is read into is a refusal rather than
        // a wrap, which would make a huge version compare as a small one.
        assert!(Version::parse("99999999999999999999999.0.0").is_none());
    }

    #[test]
    fn a_version_reads_back_as_it_was_written() {
        for text in ["1.2.3", "0.9.1", "1.2.3-rc.1", "1.2.3-alpha.beta.2", "1.0.0"] {
            assert_eq!(version(text).to_string(), text);
        }
        // The two spellings that are one version print as the fuller one, since that is
        // the spelling the channel and the release page use.
        assert_eq!(version("1.2").to_string(), "1.2.0");
        assert_eq!(version("v1.2").to_string(), "1.2.0");
    }

    #[test]
    fn a_release_candidate_is_the_one_that_is_not_a_release() {
        assert!(!version("1.2.3").is_prerelease());
        assert!(!version("1.2").is_prerelease());
        assert!(version("1.2.3-rc.1").is_prerelease());
        assert!(version("1.2.3-alpha").is_prerelease());
    }

    #[test]
    fn a_channel_file_is_read_field_by_field() {
        let text = r#"
        {
          "version": "v0.9.2",
          "channel": "labs",
          "notes": ["Fixed the table rules.", "Kept two lines on a turn."],
          "url": "https://github.com/LiJiaHua1024/rubrica/releases/tag/v0.9.2",
          "unknown": { "nested": [1, 2, { "deeper": null }], "flag": true },
          "count": 12
        }
        "#;
        let release = parse_release(text).unwrap_or_else(|| panic!("a channel file"));
        assert_eq!(release.version.to_string(), "0.9.2");
        assert_eq!(release.channel, Some(Channel::Labs));
        assert_eq!(release.notes, vec!["Fixed the table rules.", "Kept two lines on a turn."]);
        assert_eq!(
            release.url.as_deref(),
            Some("https://github.com/LiJiaHua1024/rubrica/releases/tag/v0.9.2")
        );
    }

    #[test]
    fn a_channel_file_needs_only_a_version() {
        let release = parse_release(r#"{"version":"1.0.0"}"#).unwrap_or_else(|| panic!("a channel file"));
        assert_eq!(release.version.to_string(), "1.0.0");
        assert!(release.notes.is_empty());
        assert_eq!(release.channel, None);
        assert_eq!(release.url, None);
    }

    /// An editor that marks its encoding puts a byte order mark in front of the text. The
    /// document after it is the document, so a marked file is read rather than refused.
    #[test]
    fn a_byte_order_mark_in_front_of_the_object_is_not_part_of_it() {
        let marked = "\u{feff}{\"version\":\"1.2.0\",\"notes\":[\"one\"]}";
        let release = parse_release(marked).unwrap_or_else(|| panic!("a channel file that was marked"));
        assert_eq!(release.version.to_string(), "1.2.0");
        assert_eq!(release.notes, vec!["one".to_string()]);
        // A mark anywhere it cannot start a document is not one, and is refused with the
        // rest of the text that is not a channel.
        assert!(parse_release("{\"version\":\"1.0.0\"}\u{feff}").is_none());
    }

    #[test]
    fn an_escaped_note_arrives_as_the_text_it_escaped() {
        let release = parse_release(r#"{"version":"1.0.0","notes":["a \"quote\"\nand a tab\there","\u00e9t\u00e9"]}"#)
            .unwrap_or_else(|| panic!("a channel file"));
        assert_eq!(release.notes[0], "a \"quote\"\nand a tab\there");
        assert_eq!(release.notes[1], "été");
    }

    #[test]
    fn a_file_that_is_not_a_channel_is_refused_rather_than_fatal() {
        for text in [
            "",
            "   ",
            "not json at all",
            "{",
            "{}",
            r#"{"version": 12}"#,
            r#"{"version": "abc"}"#,
            r#"{"version": "1.0.0", "notes": "one line"}"#,
            r#"{"version": "1.0.0", "notes": ["fine", 7]}"#,
            r#"{"version": "1.0.0" "notes": []}"#,
            r#"{"version": "1.0.0",}"#,
            r#"{"version": "1.0.0"} trailing"#,
            r#"["1.0.0"]"#,
            r#"{"version": "1.0.0", "notes": ["unterminated]}"#,
            r#"{"version": "1.0.0", "notes": ["raw
control"]}"#,
        ] {
            assert!(parse_release(text).is_none(), "{text:?} is not a channel file");
            // And the same read twice says the same thing: nothing here keeps state.
            assert!(parse_release(text).is_none());
        }
    }

    #[test]
    fn a_file_full_of_brackets_is_refused_rather_than_recursed_into() {
        let deep = format!("{}{}", "[".repeat(200), "]".repeat(200));
        assert!(parse_release(&deep).is_none());
        let nested = format!(r#"{{"version": "1.0.0", "notes": {}}}"#, "[".repeat(200));
        assert!(parse_release(&nested).is_none());
    }

    #[test]
    fn an_address_that_is_not_one_the_shell_can_be_handed_is_dropped() {
        let release = parse_release(r#"{"version":"1.0.0","url":"file:///C:/Windows/System32/calc.exe"}"#)
            .unwrap_or_else(|| panic!("a channel file"));
        assert_eq!(release.url, None);
        let release = parse_release(r#"{"version":"1.0.0","url":"https://"}"#)
            .unwrap_or_else(|| panic!("a channel file"));
        assert_eq!(release.url, None);
        let release = parse_release(r#"{"version":"1.0.0","url":"http://example.invalid/x"}"#)
            .unwrap_or_else(|| panic!("a channel file"));
        assert_eq!(release.url.as_deref(), Some("http://example.invalid/x"));
    }

    #[test]
    fn the_comparison_offers_releases_and_skips_candidates_on_stable() {
        let current = version("0.9.1");
        let release = |text: &str| parse_release(text).unwrap_or_else(|| panic!("a channel file"));
        let stable = release(r#"{"version":"0.9.2","notes":["a fix"],"url":"https://example.invalid/r"}"#);
        let candidate = release(r#"{"version":"0.10.0-rc.1","notes":["an experiment"]}"#);
        let older = release(r#"{"version":"0.9.0"}"#);
        let same = release(r#"{"version":"0.9.1"}"#);
        let leading = release(r#"{"version":"0.9.1-rc.2"}"#);

        assert_eq!(
            check(&current, &stable, false),
            Outcome::Newer {
                version: version("0.9.2"),
                notes: vec!["a fix".to_string()],
                url: Some("https://example.invalid/r".to_string()),
            }
        );
        // The same release on the Labs channel, because a candidate-aware reader still
        // wants to be told about releases.
        assert!(matches!(check(&current, &stable, true), Outcome::Newer { .. }));
        assert_eq!(
            check(&current, &candidate, false),
            Outcome::Skipped { version: version("0.10.0-rc.1") }
        );
        assert!(matches!(check(&current, &candidate, true), Outcome::Newer { .. }));
        assert_eq!(check(&current, &older, false), Outcome::UpToDate);
        assert_eq!(check(&current, &same, false), Outcome::UpToDate);
        assert_eq!(check(&current, &same, true), Outcome::UpToDate);
        // A candidate for the version already running is not newer than it, so it is not
        // reported as skipped either: there is nothing being held back.
        assert_eq!(check(&current, &leading, false), Outcome::UpToDate);
    }

    /// The one part of this module that touches a disk, and the part `--check-update`
    /// prints: a file written to a temporary folder, read back as lines.
    #[test]
    fn the_headless_lines_name_the_versions_the_notes_and_the_source() {
        let path = scratch("channel.json");
        std::fs::write(
            &path,
            r#"{"version":"0.10.3","channel":"stable","notes":["one","two"],"url":"https://example.invalid/r"}"#,
        )
        .expect("a channel file on disk");
        let lines = inspect(&path, Channel::Stable).expect("the channel reads");
        assert_eq!(lines[0], format!("current      : {VERSION}"));
        assert_eq!(lines[1], "latest       : 0.10.3");
        assert_eq!(lines[2], "has newer    : yes");
        assert_eq!(lines[3], "channel      : stable (pre-releases skipped)");
        assert_eq!(lines[4], format!("source       : {}", path.display()));
        assert_eq!(lines[5], "file channel : stable");
        assert_eq!(lines[6], "notes        : one");
        assert_eq!(lines[7], "notes        : two");
        assert_eq!(lines[8], "url          : https://example.invalid/r");
        assert_eq!(lines.len(), 9);

        // A candidate on the stable channel prints no as well, and says why.
        std::fs::write(&path, r#"{"version":"9.9.9-rc.1","notes":["later"]}"#).expect("a channel file on disk");
        let lines = inspect(&path, Channel::Stable).expect("the channel reads");
        assert_eq!(lines[2], "has newer    : no");
        assert!(lines.iter().any(|line| line == "skipped      : 9.9.9-rc.1 is a pre-release, and this channel skips those"), "{lines:?}");
        let lines = inspect(&path, Channel::Labs).expect("the channel reads");
        assert_eq!(lines[2], "has newer    : yes");
        assert_eq!(lines[3], "channel      : labs (pre-releases accepted)");
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn a_source_that_is_not_there_is_an_error_rather_than_an_empty_answer() {
        let missing = scratch("not-here.json");
        let error = inspect(&missing, Channel::Stable).expect_err("no file, no answer");
        assert!(error.contains("cannot read"), "{error}");
        let folder = scratch("a-folder");
        std::fs::create_dir_all(&folder).expect("a scratch folder");
        let error = inspect(&folder, Channel::Stable).expect_err("a folder is not a channel");
        assert!(error.contains("is not a file"), "{error}");
        let _ = std::fs::remove_dir_all(&folder);
        let broken = scratch("broken.json");
        std::fs::write(&broken, "{ not json").expect("a broken file on disk");
        let error = inspect(&broken, Channel::Stable).expect_err("a broken file is refused");
        assert!(error.contains("is not a release channel"), "{error}");
        let _ = std::fs::remove_file(&broken);
    }

    /// A path in the temporary folder that no other test in this process is using: the
    /// process id and a counter, since the tests run on several threads at once.
    fn scratch(name: &str) -> PathBuf {
        use std::sync::atomic::{AtomicU32, Ordering as AtomicOrdering};
        static NEXT: AtomicU32 = AtomicU32::new(0);
        let unique = NEXT.fetch_add(1, AtomicOrdering::Relaxed);
        let dir = std::env::temp_dir().join(format!("rubrica-update-{}-{unique}", std::process::id()));
        let _ = std::fs::create_dir_all(&dir);
        dir.join(name)
    }
}
