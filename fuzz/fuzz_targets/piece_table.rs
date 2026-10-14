#![no_main]

use libfuzzer_sys::fuzz_target;
use nulis_document::PieceTable;
use nulis_large_file::{MemoryAppendStore, MemorySource};
use std::sync::Arc;

fuzz_target!(|data: &[u8]| {
    let original = Arc::new(MemorySource::new(vec![0, 255, 10, 13]));
    let mut table = PieceTable::new(original, Box::new(MemoryAppendStore::new()));
    let mut model = vec![0, 255, 10, 13];
    for operation in data.chunks(7).take(1024) {
        if operation.len() < 3 { break; }
        let offset = usize::from(operation[0]) % (model.len() + 1);
        let removed = (model.len() - offset).min(usize::from(operation[1]) % 8);
        let inserted = &operation[2..];
        table.replace(offset as u64, removed as u64, inserted).expect("valid replacement");
        model.splice(offset..offset + removed, inserted.iter().copied());
        assert_eq!(table.len(), model.len() as u64);
    }
    assert_eq!(table.read_vec(0, table.len()).expect("read"), model);
    let snapshot = table.snapshot();
    assert_eq!(snapshot.document_len, model.len() as u64);
});
