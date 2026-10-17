use folitecta_core::Document;

fn tone_wav() -> Vec<u8> {
    let pcm: Vec<u8> = (0..800i32)
        .flat_map(|i| (((i as f32 * 0.2).sin() * 8000.0) as i16).to_le_bytes())
        .collect();
    let mut out = Vec::new();
    out.extend_from_slice(b"RIFF");
    out.extend_from_slice(&(36 + pcm.len() as u32).to_le_bytes());
    out.extend_from_slice(b"WAVEfmt ");
    out.extend_from_slice(&16u32.to_le_bytes());
    out.extend_from_slice(&1u16.to_le_bytes());
    out.extend_from_slice(&1u16.to_le_bytes());
    out.extend_from_slice(&8000u32.to_le_bytes());
    out.extend_from_slice(&16000u32.to_le_bytes());
    out.extend_from_slice(&2u16.to_le_bytes());
    out.extend_from_slice(&16u16.to_le_bytes());
    out.extend_from_slice(b"data");
    out.extend_from_slice(&(pcm.len() as u32).to_le_bytes());
    out.extend_from_slice(&pcm);
    out
}

#[test]
fn sound_and_video_comments() {
    let mut doc = Document::new_blank(1, 595.0, 842.0).unwrap();
    let wav = tone_wav();
    doc.add_sound(0, (100.0, 700.0), &wav, "Reviewer").unwrap();
    let fake_video = b"\x00\x00\x00\x18ftypmp42 not a real movie".to_vec();
    doc.add_media(
        0,
        [200.0, 500.0, 400.0, 650.0],
        "clip.mp4",
        fake_video.clone(),
        "Reviewer",
    )
    .unwrap();
    assert!(
        doc.add_media(0, [0.0, 0.0, 50.0, 50.0], "notes.txt", vec![1], "R")
            .is_err()
    );
    let items = doc.media_items();
    assert_eq!(items.len(), 2);
    assert_eq!(items[0].kind, "sound");
    assert_eq!(items[1].kind, "video");
    assert_eq!(items[1].mime, "video/mp4");
    // The sound plays as the same WAV; the video as the embedded file.
    assert_eq!(doc.media_data(0, 0).unwrap().2, wav);
    assert_eq!(
        doc.media_data(0, 1).unwrap(),
        (
            "clip.mp4".to_string(),
            "video/mp4".to_string(),
            fake_video.clone()
        )
    );
    // Saved and opened again.
    let path = std::env::temp_dir().join("folitecta-media-test.pdf");
    doc.save_as(&path).unwrap();
    let again = Document::open(&path).unwrap();
    let _ = std::fs::remove_file(&path);
    assert_eq!(again.media_items().len(), 2);
    assert_eq!(again.media_data(0, 1).unwrap().2, fake_video);
    assert_eq!(again.media_data(0, 0).unwrap().2, wav);
    // Title and description; moving keeps the media.
    doc.set_media_text(0, 1, "Interview", "Short interview").unwrap();
    assert_eq!(doc.media_items()[1].title, "Interview");
    doc.move_annotation(0, 1, 10.0, 10.0).unwrap();
    assert_eq!(doc.media_data(0, 1).unwrap().2.len(), fake_video.len());
    assert!(doc.set_media_text(0, 5, "x", "y").is_err());
}
