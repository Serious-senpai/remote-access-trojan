use alloc::sync::Arc;
use alloc::vec::Vec;

use crate::utils;

#[derive(Debug)]
pub struct PatchResult {
    pub offset: usize,
    pub original: Vec<u8>,
}

pub struct PatternFinder {
    _original: &'static [u8],
}

impl PatternFinder {
    pub fn new(original: &'static [u8]) -> Self {
        Self {
            _original: original,
        }
    }

    pub fn len(&self) -> usize {
        self._original.len()
    }

    pub fn find_offset(&self, buffer: &[u8]) -> Option<usize> {
        utils::find_pattern(buffer, self._original)
    }

    pub fn find_mut<'a>(&self, buffer: &'a mut [u8]) -> Option<&'a mut [u8]> {
        self.find_offset(buffer).map(|i| &mut buffer[i..])
    }

    pub fn find_ref<'a>(&self, buffer: &'a [u8]) -> Option<&'a [u8]> {
        self.find_offset(buffer).map(|i| &buffer[i..])
    }
}

pub struct PatternPatcher {
    _finder: PatternFinder,
    _patched: &'static [u8],
    _postfix: Arc<dyn Fn(&Self, &mut [u8])>,
}

impl PatternPatcher {
    pub fn new(
        original: &'static [u8],
        patched: &'static [u8],
        postfix: Arc<dyn Fn(&Self, &mut [u8])>,
    ) -> Self {
        Self {
            _finder: PatternFinder::new(original),
            _patched: patched,
            _postfix: postfix,
        }
    }

    pub fn patch(&self, buffer: &mut [u8]) -> Option<PatchResult> {
        match self._finder.find_offset(buffer) {
            Some(offset) => {
                let modify = &mut buffer[offset..];

                let original = modify[..self._patched.len().max(self._finder.len())].to_vec();
                modify[..self._patched.len()].copy_from_slice(self._patched);
                (self._postfix)(self, modify);

                Some(PatchResult { offset, original })
            }
            None => None,
        }
    }
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

pub struct VariablePatternPatcher<const SIZE: usize> {
    _patchers: Vec<PatternPatcher>,
}

impl<const SIZE: usize> VariablePatternPatcher<SIZE> {
    pub fn new(
        original: &'static [[u8; SIZE]],
        patched: &'static [u8],
        postfix: Arc<dyn Fn(&PatternPatcher, &mut [u8])>,
    ) -> Self {
        Self {
            _patchers: original
                .iter()
                .map(|orig| PatternPatcher::new(orig, patched, postfix.clone()))
                .collect(),
        }
    }

    pub fn patch(&self, buffer: &mut [u8]) -> Option<PatchResult> {
        for patcher in &self._patchers {
            if let Some(result) = patcher.patch(buffer) {
                return Some(result);
            }
        }

        None
    }
}
