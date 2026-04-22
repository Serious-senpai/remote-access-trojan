use alloc::vec::Vec;

use crate::utils;

pub struct PatternFinder {
    _original: &'static [u8],
}

impl PatternFinder {
    pub fn new(original: &'static [u8]) -> Self {
        Self {
            _original: original,
        }
    }

    // pub fn len(&self) -> usize {
    //     self._original.len()
    // }

    pub fn find_offset(&self, buffer: &[u8]) -> Option<usize> {
        utils::find_pattern(buffer, self._original)
    }

    // pub fn find_mut<'a>(&self, buffer: &'a mut [u8]) -> Option<&'a mut [u8]> {
    //     self.find_offset(buffer).map(|i| &mut buffer[i..])
    // }

    // pub fn find_ref<'a>(&self, buffer: &'a [u8]) -> Option<&'a [u8]> {
    //     self.find_offset(buffer).map(|i| &buffer[i..])
    // }
}

pub struct VariablePatternFinder<const SIZE: usize> {
    _patterns: Vec<PatternFinder>,
}

impl<const SIZE: usize> VariablePatternFinder<SIZE> {
    pub fn new(original: &'static [[u8; SIZE]]) -> Self {
        Self {
            _patterns: original
                .iter()
                .map(|pattern| PatternFinder::new(pattern))
                .collect(),
        }
    }

    pub fn find_offset(&self, buffer: &[u8]) -> Option<usize> {
        for finder in &self._patterns {
            if let Some(r) = finder.find_offset(buffer) {
                return Some(r);
            }
        }

        None
    }

    pub fn find_mut<'a>(&self, buffer: &'a mut [u8]) -> Option<&'a mut [u8]> {
        self.find_offset(buffer).map(|i| &mut buffer[i..])
    }

    pub fn find_ref<'a>(&self, buffer: &'a [u8]) -> Option<&'a [u8]> {
        self.find_offset(buffer).map(|i| &buffer[i..])
    }
}
