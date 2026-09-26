#[derive(Clone, Debug)]
pub struct ChunkDigest {
    pub offset: u64,
    pub length: usize,
    pub digest: blake3::Hash,
}
