use std::path::{Path, PathBuf};

use thiserror::Error;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NamingContext {
    year: u16,
    month: u8,
    day: u8,
    hour: u8,
    minute: u8,
    second: u8,
    hostname: String,
}

impl NamingContext {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        year: u16,
        month: u8,
        day: u8,
        hour: u8,
        minute: u8,
        second: u8,
        hostname: impl Into<String>,
    ) -> Self {
        Self {
            year,
            month,
            day,
            hour,
            minute,
            second,
            hostname: hostname.into(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NameAllocator {
    pattern: String,
    minimum_width: usize,
    extension: String,
    context: NamingContext,
    has_sequence: bool,
}

impl NameAllocator {
    pub fn new(
        pattern: impl Into<String>,
        minimum_width: u8,
        extension: impl Into<String>,
        context: NamingContext,
    ) -> Result<Self, NamingError> {
        let pattern = pattern.into();
        let extension = extension.into();
        if pattern.is_empty() {
            return Err(NamingError::EmptyPattern);
        }
        if minimum_width == 0 {
            return Err(NamingError::InvalidMinimumWidth);
        }
        if extension.is_empty() || extension.starts_with('.') {
            return Err(NamingError::InvalidFilename(extension));
        }

        let has_sequence = validate_tokens(&pattern)?;
        let allocator = Self {
            pattern,
            minimum_width: usize::from(minimum_width),
            extension,
            context,
            has_sequence,
        };
        allocator.render_filename(1)?;
        Ok(allocator)
    }

    pub fn render_filename(&self, sequence: u64) -> Result<String, NamingError> {
        let mut rendered = String::with_capacity(self.pattern.len() + self.extension.len() + 16);
        let mut characters = self.pattern.chars();
        while let Some(character) = characters.next() {
            if character != '%' {
                rendered.push(character);
                continue;
            }

            let token = characters
                .next()
                .ok_or_else(|| NamingError::UnsupportedToken("%".to_owned()))?;
            match token {
                '%' => rendered.push('%'),
                'c' => rendered.push_str(&render_sequence(sequence, self.minimum_width)),
                'Y' => rendered.push_str(&format!("{:04}", self.context.year)),
                'm' => rendered.push_str(&format!("{:02}", self.context.month)),
                'd' => rendered.push_str(&format!("{:02}", self.context.day)),
                'H' => rendered.push_str(&format!("{:02}", self.context.hour)),
                'M' => rendered.push_str(&format!("{:02}", self.context.minute)),
                'S' => rendered.push_str(&format!("{:02}", self.context.second)),
                'h' => rendered.push_str(&self.context.hostname),
                _ => return Err(NamingError::UnsupportedToken(format!("%{token}"))),
            }
        }
        if matches!(rendered.as_str(), "." | "..") || rendered.ends_with(['.', ' ']) {
            return Err(NamingError::InvalidFilename(rendered));
        }
        let filename = format!("{rendered}.{}", self.extension);
        validate_portable_filename(&filename)?;
        Ok(filename)
    }

    pub fn path_for(
        &self,
        directory: impl AsRef<Path>,
        sequence: u64,
    ) -> Result<PathBuf, NamingError> {
        Ok(directory.as_ref().join(self.render_filename(sequence)?))
    }

    pub fn has_sequence(&self) -> bool {
        self.has_sequence
    }
}

pub fn render_sequence(sequence: u64, minimum_width: usize) -> String {
    format!("{sequence:0minimum_width$}")
}

#[derive(Debug, Error)]
pub enum NamingError {
    #[error("filename pattern cannot be empty")]
    EmptyPattern,
    #[error("minimum sequence width must be greater than zero")]
    InvalidMinimumWidth,
    #[error("filename token {0} is not supported")]
    UnsupportedToken(String),
    #[error("filename {0:?} is not portable")]
    InvalidFilename(String),
    #[error("the filename sequence is exhausted")]
    SequenceExhausted,
}

fn validate_tokens(pattern: &str) -> Result<bool, NamingError> {
    let mut has_sequence = false;
    let mut characters = pattern.chars();
    while let Some(character) = characters.next() {
        if character != '%' {
            continue;
        }
        let token = characters
            .next()
            .ok_or_else(|| NamingError::UnsupportedToken("%".to_owned()))?;
        if !matches!(token, '%' | 'c' | 'Y' | 'm' | 'd' | 'H' | 'M' | 'S' | 'h') {
            return Err(NamingError::UnsupportedToken(format!("%{token}")));
        }
        has_sequence |= token == 'c';
    }
    Ok(has_sequence)
}

fn validate_portable_filename(filename: &str) -> Result<(), NamingError> {
    let invalid_character = filename.chars().any(|character| {
        character.is_control()
            || matches!(
                character,
                '<' | '>' | ':' | '"' | '/' | '\\' | '|' | '?' | '*'
            )
    });
    let invalid_shape = filename == "."
        || filename == ".."
        || filename.ends_with(['.', ' '])
        || filename.len() > MAX_PORTABLE_FILENAME_BYTES;
    let reserved = is_windows_reserved_component(filename);

    if invalid_character || invalid_shape || reserved {
        return Err(NamingError::InvalidFilename(filename.to_owned()));
    }
    Ok(())
}

/// Portable target-component limit. Storage temporary names are generated
/// independently and never prepend text to this target filename.
pub const MAX_PORTABLE_FILENAME_BYTES: usize = 255;

pub(crate) fn is_windows_reserved_component(component: &str) -> bool {
    let stem = component
        .split('.')
        .next()
        .unwrap_or(component)
        .trim_end_matches(['.', ' '])
        .to_ascii_uppercase();
    matches!(stem.as_str(), "CON" | "PRN" | "AUX" | "NUL")
        || reserved_numbered_name(&stem, "COM")
        || reserved_numbered_name(&stem, "LPT")
}

fn reserved_numbered_name(stem: &str, prefix: &str) -> bool {
    stem.strip_prefix(prefix).is_some_and(|suffix| {
        matches!(suffix, "¹" | "²" | "³")
            || suffix
                .parse::<u8>()
                .is_ok_and(|number| (1..=9).contains(&number))
    })
}
