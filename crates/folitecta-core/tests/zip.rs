use folitecta_core::zip::{ZipEntry, read, write};

#[test]
fn archives_read_back_what_was_written() {
    let text = "Folitecta ".repeat(500).into_bytes();
    // Bytes that do not compress are stored as they are.
    let mut noise = Vec::new();
    let mut x: u32 = 12345;
    for _ in 0..4000 {
        x = x.wrapping_mul(1_103_515_245).wrapping_add(12_345);
        noise.push((x >> 16) as u8);
    }
    let entries = vec![
        ZipEntry::new("report.txt", text.clone()),
        ZipEntry::new("folder/noise.bin", noise.clone()),
        ZipEntry::new("Relazione è pronta.pdf", b"%PDF-1.7".to_vec()),
        ZipEntry::new("empty.txt", Vec::new()),
    ];
    let zip = write(&entries).unwrap();
    assert_eq!(&zip[..4], b"PK\x03\x04");
    // The repeated text was deflated.
    assert!(zip.len() < text.len() + noise.len());
    assert_eq!(read(&zip).unwrap(), entries);
}

#[test]
fn damaged_archives_are_refused() {
    let zip = write(&[ZipEntry::new("a.txt", b"hello hello hello hello".to_vec())]).unwrap();
    assert!(read(b"not an archive at all, just some text").is_err());
    let mut broken = zip.clone();
    // A changed byte of the compressed data fails the checksum.
    broken[40] ^= 0xFF;
    assert!(read(&broken).is_err());
    assert!(read(&zip[..zip.len() - 10]).is_err());
    assert!(write(&[ZipEntry::new("", Vec::new())]).is_err());
}
