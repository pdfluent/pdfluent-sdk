use super::CFFError;

pub struct ArgumentsStack<'a> {
    pub data: &'a mut [f64],
    pub len: usize,
    pub max_len: usize,
}

impl<'a> ArgumentsStack<'a> {
    #[inline]
    pub fn len(&self) -> usize {
        self.len
    }

    #[inline]
    pub fn is_empty(&self) -> bool {
        self.len == 0
    }

    #[inline]
    pub fn push(&mut self, n: f64) -> Result<(), CFFError> {
        if self.len == self.max_len {
            Err(CFFError::ArgumentsStackLimitReached)
        } else {
            self.data[self.len] = n;
            self.len += 1;
            Ok(())
        }
    }

    #[inline]
    pub fn at(&self, index: usize) -> f64 {
        self.data[index]
    }

    #[inline]
    pub fn pop(&mut self) -> Result<f64, CFFError> {
        if self.is_empty() {
            return Err(CFFError::InvalidArgumentsStackLength);
        }

        self.len -= 1;
        Ok(self.data[self.len])
    }

    #[inline]
    pub fn reverse(&mut self) {
        if self.is_empty() {
            return;
        }

        // Reverse only the actual data and not the whole stack.
        let (first, _) = self.data.split_at_mut(self.len);
        first.reverse();
    }

    #[inline]
    pub fn clear(&mut self) {
        self.len = 0;
    }
}

impl core::fmt::Debug for ArgumentsStack<'_> {
    fn fmt(&self, f: &mut core::fmt::Formatter) -> core::fmt::Result {
        f.debug_list().entries(&self.data[..self.len]).finish()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pop_empty_stack_returns_error() {
        let mut data = [0.0; 1];
        let mut stack = ArgumentsStack {
            data: &mut data,
            len: 0,
            max_len: 1,
        };

        assert_eq!(stack.pop(), Err(CFFError::InvalidArgumentsStackLength));
    }
}
