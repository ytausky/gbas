struct StringCodebase {
    bufs: Vec<SrcBuf>,
}

struct SrcBuf {
    src: String,
    start_index: usize,
}

struct BufId(usize);

impl StringCodebase {
    fn new() -> StringCodebase {
        StringCodebase { bufs: Vec::new() }
    }

    fn add_src_buf(&mut self, src: String) -> BufId {
        let buf_id = BufId(self.bufs.len());
        let start_index = match self.bufs.last() {
            Some(ref src_buf) => src_buf.start_index + src_buf.src.len(),
            None => 0,
        };
        self.bufs.push(SrcBuf { src, start_index });
        buf_id
    }

    fn buf(&self, buf_id: BufId) -> SrcBufIter {
        let src_buf = &self.bufs[buf_id.0];
        SrcBufIter {
            char_indices: src_buf.src.char_indices(),
            start_index: src_buf.start_index,
        }
    }
}

use std::str::CharIndices;

struct SrcBufIter<'a> {
    char_indices: CharIndices<'a>,
    start_index: usize,
}

impl<'a> Iterator for SrcBufIter<'a> {
    type Item = (usize, char);

    fn next(&mut self) -> Option<Self::Item> {
        self.char_indices
            .next()
            .map(|(index, ch)| (self.start_index + index, ch))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn iterate_src() {
        let mut codebase = StringCodebase::new();
        let src = "src";
        let buf_id = codebase.add_src_buf(String::from(src));
        let mut iter = codebase.buf(buf_id);
        assert_eq!(iter.next(), Some((0, 's')));
        assert_eq!(iter.next(), Some((1, 'r')));
        assert_eq!(iter.next(), Some((2, 'c')));
        assert_eq!(iter.next(), None);
    }

    #[test]
    fn second_buffer_disjoint_from_first() {
        let mut codebase = StringCodebase::new();
        let src_a = "some source string";
        let buf_id_a = codebase.add_src_buf(String::from(src_a));
        let end_a = {
            let mut iter = codebase.buf(buf_id_a);
            let mut end = None;
            while let Some((new_end, ch)) = iter.next() {
                end = Some(new_end + ch.len_utf8())
            }
            end
        };
        let src_b = "another string";
        let buf_id_b = codebase.add_src_buf(String::from(src_b));
        let start_b = codebase.buf(buf_id_b).next().map(|(idx, _)| idx);
        assert_eq!(end_a, start_b)
    }
}