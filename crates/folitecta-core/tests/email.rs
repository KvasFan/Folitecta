use folitecta_core::email::{EmailLabels, read_email, write_eml_draft};
use folitecta_core::{Document, TextLayout};

const MESSAGE: &str = "From: Ada Rossi <ada@example.org>\r
To: Team <team@example.org>, bob@example.org\r
Cc: carla@example.org\r
Subject: =?UTF-8?Q?Verbale_della_riunione_=E2=80=94_luned=C3=AC?=\r
Date: Mon, 12 Oct 2026 09:30:00 +0200\r
MIME-Version: 1.0\r
Content-Type: multipart/mixed; boundary=\"outer\"\r
\r
--outer\r
Content-Type: multipart/alternative; boundary=\"inner\"\r
\r
--inner\r
Content-Type: text/plain; charset=utf-8\r
Content-Transfer-Encoding: quoted-printable\r
\r
Ciao a tutti,=0D=0A=0D=0Ail verbale =C3=A8 allegato. Perch=C3=A9 non lo leggete?\r
\r
--inner\r
Content-Type: text/html; charset=utf-8\r
\r
<p>Ciao a tutti</p>\r
--inner--\r
\r
--outer\r
Content-Type: application/pdf; name=\"verbale.pdf\"\r
Content-Disposition: attachment; filename=\"verbale.pdf\"\r
Content-Transfer-Encoding: base64\r
\r
JVBERi0xLjcKJcTl8uXr\r
\r
--outer--\r
";

#[test]
fn messages_are_read_with_their_attachments() {
    let msg = read_email(MESSAGE.as_bytes()).unwrap();
    assert_eq!(msg.subject, "Verbale della riunione — lunedì");
    assert_eq!(msg.from, "Ada Rossi <ada@example.org>");
    assert!(msg.to.contains("Team <team@example.org>") && msg.to.contains("bob@example.org"));
    assert_eq!(msg.cc, "carla@example.org");
    assert_eq!(msg.date, "2026-10-12 09:30 +02:00");
    assert!(msg.body.contains("il verbale è allegato"), "{:?}", msg.body);
    assert_eq!(msg.attachments.len(), 1);
    assert_eq!(msg.attachments[0].0, "verbale.pdf");
    assert!(msg.attachments[0].1.starts_with(b"%PDF-1.7"));
    assert!(read_email(b"just some text without any header").is_err());
}

#[test]
fn messages_become_documents_with_embedded_attachments() {
    let msg = read_email(MESSAGE.as_bytes()).unwrap();
    let mut doc = Document::from_email(&msg, &EmailLabels::default(), TextLayout::default()).unwrap();
    assert_eq!(doc.page_count(), 1);
    let text = doc.page_text(0);
    assert!(text.contains("Verbale della riunione"), "{text}");
    assert!(
        text.contains("From:") && text.contains("ada@example.org"),
        "{text}"
    );
    assert!(text.contains("verbale.pdf"), "{text}");
    let attachments = doc.attachments();
    assert_eq!(attachments.len(), 1);
    assert_eq!(attachments[0].name, "verbale.pdf");
    assert!(
        doc.attachment_data(&attachments[0])
            .unwrap()
            .starts_with(b"%PDF-1.7")
    );
    assert_eq!(doc.title(), "Verbale della riunione — lunedì");
    // A long message goes on as many pages as it needs, and saves.
    let mut long = msg.clone();
    long.body = "Una riga del messaggio.\n".repeat(200);
    let doc2 = Document::from_email(&long, &EmailLabels::default(), TextLayout::default()).unwrap();
    assert!(doc2.page_count() >= 3);
    let path = std::env::temp_dir().join("folitecta-email-test.pdf");
    doc.save_copy(&path).unwrap();
    assert_eq!(Document::open(&path).unwrap().attachments().len(), 1);
    let _ = std::fs::remove_file(&path);
}

#[test]
fn drafts_carry_their_files() {
    let files = vec![
        ("Relazione è pronta.pdf".to_string(), b"%PDF-1.7 a".to_vec()),
        ("dati.xfdf".to_string(), b"<xfdf/>".to_vec()),
    ];
    let eml = write_eml_draft("Relazione — ottobre", "Ecco i file.", &files);
    let text = String::from_utf8_lossy(&eml);
    assert!(text.starts_with("X-Unsent: 1\r\n"));
    let back = read_email(&eml).unwrap();
    assert_eq!(back.subject, "Relazione — ottobre");
    assert_eq!(back.body, "Ecco i file.");
    assert_eq!(back.attachments, files);
}

#[test]
fn long_subjects_and_names_stay_within_the_line_limits() {
    let subject = "Relazione dell'ufficio tecnico — verifiche di ottobre, città e università ".repeat(12);
    let long_name = format!("{}.pdf", "Città dell'àrea — perché sì ".repeat(6).trim());
    let files = vec![
        (long_name.clone(), b"%PDF-1.7 a".to_vec()),
        ("plain \"quoted\" name.pdf".to_string(), b"%PDF-1.7 b".to_vec()),
    ];
    let eml = write_eml_draft(&subject, "Ecco i file.", &files);
    let text = String::from_utf8(eml.clone()).unwrap();
    // RFC 5322: no line over 998 characters; RFC 2047: encoded words of at most 75.
    for line in text.split("\r\n") {
        assert!(line.len() <= 998, "line of {} characters", line.len());
        for word in line.split(' ').filter(|w| w.starts_with("=?")) {
            assert!(word.trim_matches('"').len() <= 75, "encoded word of {} characters", word.len());
        }
    }
    // RFC 2231 for the file name of a disposition.
    assert!(text.contains("filename*0*=UTF-8''"));
    let back = read_email(&eml).unwrap();
    assert_eq!(back.subject, subject.trim());
    assert_eq!(back.attachments, files);
}
