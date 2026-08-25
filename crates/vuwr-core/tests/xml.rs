//! XML-specific tests: tree structure, table eligibility, roundtrip.

use vuwr_core::{Document, FormatHint, Node, PathSeg};

#[test]
fn xml_auto_detected() {
    let doc = Document::parse(b"<root/>", FormatHint::Auto).unwrap();
    assert!(doc.is_xml());
}

#[test]
fn xml_roundtrip_preserves_comments() {
    let doc = Document::parse(b"<root><!-- keep --><child/></root>", FormatHint::Auto).unwrap();
    let out = doc.serialize();
    assert_eq!(out, b"<root><!-- keep --><child/></root>");
}

#[test]
fn xml_roundtrip_preserves_attribute_order() {
    let doc = Document::parse(b"<root z=\"1\" a=\"2\"/>", FormatHint::Auto).unwrap();
    let out = doc.serialize();
    assert_eq!(out, b"<root z=\"1\" a=\"2\"/>");
}

#[test]
fn xml_roundtrip_preserves_self_closing() {
    let doc = Document::parse(b"<br/>", FormatHint::Auto).unwrap();
    let out = doc.serialize();
    assert_eq!(out, b"<br/>");
}

#[test]
fn xml_roundtrip_preserves_empty_element() {
    let doc = Document::parse(b"<p></p>", FormatHint::Auto).unwrap();
    let out = doc.serialize();
    assert_eq!(out, b"<p></p>");
}

#[test]
fn xml_roundtrip_preserves_xml_decl() {
    let doc = Document::parse(
        b"<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<root/>",
        FormatHint::Auto,
    )
    .unwrap();
    let out = doc.serialize();
    assert_eq!(out, b"<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<root/>");
}

#[test]
fn xml_tree_structure() {
    let doc = Document::parse(b"<root><child>text</child></root>", FormatHint::Auto).unwrap();
    let xml = doc.as_xml().unwrap();
    match xml.root() {
        Node::Element(e) => {
            assert_eq!(e.tag, "root");
            assert_eq!(e.children.len(), 1);
            if let Node::Element(child) = &e.children[0] {
                assert_eq!(child.tag, "child");
            } else {
                panic!("expected child element");
            }
        }
        _ => panic!("expected element"),
    }
}

#[test]
fn xml_table_eligible_repeated_siblings() {
    let doc = Document::parse(
        b"<items><item name=\"a\"/><item name=\"b\"/></items>",
        FormatHint::Auto,
    )
    .unwrap();
    assert!(doc.xml_table_eligible());
}

#[test]
fn xml_not_table_eligible_different_tags() {
    let doc = Document::parse(b"<root><a/><b/></root>", FormatHint::Auto).unwrap();
    assert!(!doc.xml_table_eligible());
}

// --- Table shape regressions ---
//
// `root()` used to return the first child, which for a declared document is
// the XML declaration — so every real-world file looked shapeless. And
// eligibility counted whitespace `Text` nodes as children, so any
// pretty-printed file was ineligible too.

#[test]
fn table_eligible_with_declaration_and_comments() {
    let src = br#"<?xml version="1.0"?>
<!-- a leading comment -->
<items>
  <item name="a"/>
  <item name="b"/>
</items>"#;
    let doc = Document::parse(src, FormatHint::Auto).unwrap();
    assert!(
        doc.xml_table_eligible(),
        "a declared, pretty-printed document is still table-shaped"
    );
}

#[test]
fn table_headers_and_cells_span_attributes_then_children() {
    let src = br#"<?xml version="1.0"?>
<rows>
  <row id="1" kind="a"><name>Alice</name><note/></row>
  <row id="2" kind="b"><name>Bob</name><note>hi</note></row>
</rows>"#;
    let doc = Document::parse(src, FormatHint::Auto).unwrap();
    let xml = doc.as_xml().unwrap();

    assert_eq!(xml.table_headers(), vec!["id", "kind", "name", "note"]);
    assert_eq!(xml.row_elements().len(), 2);

    assert_eq!(xml.table_cell(0, 0).as_deref(), Some("1"));
    assert_eq!(xml.table_cell(0, 1).as_deref(), Some("a"));
    assert_eq!(xml.table_cell(0, 2).as_deref(), Some("Alice"));
    // `<note/>` is empty: this used to index children[0] and panic.
    assert_eq!(xml.table_cell(0, 3).as_deref(), Some(""));
    assert_eq!(xml.table_cell(1, 3).as_deref(), Some("hi"));

    assert_eq!(xml.table_cell(9, 0), None, "out of range must not panic");
}

#[test]
fn document_element_skips_the_prolog() {
    let doc = Document::parse(br#"<?xml version="1.0"?><r><a/></r>"#, FormatHint::Auto).unwrap();
    match doc.as_xml().unwrap().root() {
        vuwr_core::Node::Element(e) => assert_eq!(e.tag, "r"),
        other => panic!("root should be the document element, got {other:?}"),
    }
}

#[test]
fn empty_document_is_an_error_not_a_panic() {
    assert!(Document::parse(b"   ", FormatHint::Xml).is_err());
}

// --- Well-formedness ---
//
// The parser used to accept any closing tag, and to silently rewrite an
// unclosed element as self-closing — which changed the document rather
// than rejecting it.

#[test]
fn a_mismatched_closing_tag_is_an_error() {
    let Err(e) = Document::parse(b"<r><a></r>", FormatHint::Xml) else {
        panic!("`<r><a></r>` must not parse");
    };
    let msg = e.to_string();
    assert!(msg.contains("<a>") && msg.contains("</r>"), "{msg}");
}

/// This is the important one: it was not merely accepted, it was silently
/// turned into `<a/>` and the text content was discarded on save.
#[test]
fn an_unclosed_element_is_an_error_not_a_rewrite() {
    for src in [&b"<r><a>text"[..], &b"<r><a>text</a>"[..], &b"<a>"[..]] {
        assert!(
            Document::parse(src, FormatHint::Xml).is_err(),
            "{:?} must not parse",
            String::from_utf8_lossy(src)
        );
    }
}

#[test]
fn well_formed_documents_still_parse() {
    for src in [
        &b"<r><a>text</a></r>"[..],
        &b"<r><a/></r>"[..],
        &b"<r><a></a></r>"[..],
        &b"<?xml version=\"1.0\"?><r><!-- c --><a x=\"1\"/></r>"[..],
        &b"<r>\n  <a>1</a>\n  <a>2</a>\n</r>"[..],
    ] {
        Document::parse(src, FormatHint::Xml)
            .unwrap_or_else(|e| panic!("{:?}: {e}", String::from_utf8_lossy(src)));
    }
}

#[test]
fn errors_carry_a_position() {
    let src = b"<r>\n  <a>\n</r>";
    let Err(e) = Document::parse(src, FormatHint::Xml) else {
        panic!("expected an error");
    };
    let located = e.located(src);
    assert!(
        located.starts_with("2:") || located.starts_with("3:"),
        "points into the document: {located}"
    );
}

// --- Constructs real files use ---
//
// A Google Shopping feed failed to open at all: `<![CDATA[...]]>` was read
// as a tag name, so every value in the file was a parse error. The corpus
// now carries `feed.xml` exercising these, and the round-trip test covers
// it; these assert the meaning rather than just the bytes.

fn xml(src: &str) -> Document {
    Document::parse(src.as_bytes(), FormatHint::Xml).unwrap()
}
fn out(d: &Document) -> String {
    String::from_utf8(d.serialize()).unwrap()
}

#[test]
fn cdata_sections_parse_and_survive() {
    let src = "<r><id><![CDATA[WRZ990100SI]]></id></r>";
    let d = xml(src);
    assert_eq!(out(&d), src);
}

/// The point of CDATA is that its contents are not markup. Angle brackets
/// and ampersands inside must not be treated as tags or entities, and must
/// not be escaped on the way out.
#[test]
fn cdata_contents_are_not_markup() {
    let src = "<r><t><![CDATA[a <not a tag> & not an entity]]></t></r>";
    assert_eq!(out(&xml(src)), src);
}

/// `]]` may appear inside a section as long as it is not `]]>`.
#[test]
fn cdata_ending_is_only_the_full_terminator() {
    let src = "<r><t><![CDATA[a ]] b ]]]></t></r>";
    assert_eq!(out(&xml(src)), src);
}

#[test]
fn an_unterminated_cdata_is_an_error() {
    assert!(Document::parse(b"<r><![CDATA[oops</r>", FormatHint::Xml).is_err());
}

/// An element's value is its text whether written plainly or wrapped in
/// CDATA — a feed that wraps everything would otherwise show empty rows.
#[test]
fn cdata_reads_as_the_elements_value() {
    let d = xml("<rows><row><name><![CDATA[Alice]]></name></row></rows>");
    assert_eq!(
        d.as_xml().unwrap().table_cell(0, 0).as_deref(),
        Some("Alice")
    );
}

#[test]
fn editing_a_cdata_value_keeps_it_a_cdata_section() {
    let mut d = xml("<rows><row><name><![CDATA[Alice]]></name></row></rows>");
    d.set_cell(0, 0, "Bob").unwrap();
    assert_eq!(
        out(&d),
        "<rows><row><name><![CDATA[Bob]]></name></row></rows>",
        "rewriting it as plain text would change how the document escapes"
    );
}

#[test]
fn doctype_declarations_survive() {
    let src = "<!DOCTYPE html><html><body/></html>";
    assert_eq!(out(&xml(src)), src);
}

/// An internal subset contains `>` inside brackets, so the scan cannot
/// stop at the first one.
#[test]
fn doctype_with_an_internal_subset_survives() {
    let src = "<!DOCTYPE rss [\n  <!ENTITY brand \"Acme\">\n]>\n<rss/>";
    assert_eq!(out(&xml(src)), src);
}

/// `<?xml-stylesheet ...?>` is a processing instruction. Matching it as
/// the XML declaration threw its contents away and wrote back
/// `<?xml version=""?>`.
#[test]
fn a_pi_starting_with_xml_is_not_the_declaration() {
    let src = "<?xml version=\"1.0\"?><?xml-stylesheet type=\"text/xsl\" href=\"f.xsl\"?><r/>";
    assert_eq!(out(&xml(src)), src);
}

#[test]
fn namespaced_tags_and_attributes_survive() {
    let src = "<rss xmlns:g=\"http://base.google.com/ns/1.0\"><g:id>1</g:id></rss>";
    assert_eq!(out(&xml(src)), src);
}

/// Entities outside CDATA are left exactly as written: decoding them would
/// change the bytes on save.
#[test]
fn entities_are_preserved_verbatim() {
    let src = "<r><t>&amp; &lt; &gt; &quot; &apos; &#169; &#x2014;</t></r>";
    assert_eq!(out(&xml(src)), src);
}

#[test]
fn single_and_double_quoted_attributes_keep_their_quotes() {
    let src = "<r a='one' b=\"two\"/>";
    assert_eq!(out(&xml(src)), src);
}

#[test]
fn mixed_content_survives() {
    let src = "<p>text <b>bold</b> tail</p>";
    assert_eq!(out(&xml(src)), src);
}

/// Real feeds contain `<tag >` and `<tag />`. Dropping the space rewrote
/// bytes nobody asked us to touch — 1,377 of them in a 6.7 MB feed.
#[test]
fn whitespace_inside_a_tag_survives() {
    for src in [
        "<r><g:item_group_id ><![CDATA[3046]]></g:item_group_id ></r>",
        "<r><a /></r>",
        "<r><a\n  b=\"1\"\n/></r>",
        "<r><a b=\"1\"   ></a></r>",
    ] {
        assert_eq!(out(&xml(src)), src, "{src}");
    }
}

// --- Finding the rows in a nested document ---
//
// A Google feed wraps its records: `<rss><channel><item>…`. Looking only
// at the document element's children made a 2,277-item feed one row of
// 2,277 columns all called "item".

#[test]
fn a_wrapped_feed_finds_its_items() {
    let src = "<rss><channel>\
        <item><id>1</id><title>One</title></item>\
        <item><id>2</id><title>Two</title></item>\
        <item><id>3</id><title>Three</title></item>\
        </channel></rss>";
    let d = xml(src);
    let x = d.as_xml().unwrap();
    assert_eq!(x.row_elements().len(), 3, "three items, not one channel");
    assert_eq!(x.table_headers(), vec!["id", "title"]);
    assert_eq!(x.table_cell(1, 1).as_deref(), Some("Two"));
    assert!(d.xml_table_eligible());
}

#[test]
fn deeper_wrapping_still_finds_the_rows() {
    let src = "<a><b><c><row><v>1</v></row><row><v>2</v></row></c></b></a>";
    let d = xml(src);
    assert_eq!(d.as_xml().unwrap().row_elements().len(), 2);
}

/// The shallowest repeating level wins: `<rows><row><name>` is rows of
/// `row`, not rows of `name`.
#[test]
fn the_shallowest_repeating_level_is_the_table() {
    let d = xml("<rows><row><name>Alice</name><age>30</age></row></rows>");
    let x = d.as_xml().unwrap();
    assert_eq!(x.row_elements().len(), 1);
    assert_eq!(x.table_headers(), vec!["name", "age"]);
    assert_eq!(x.table_cell(0, 0).as_deref(), Some("Alice"));
}

/// Editing must reach the right node once the rows are nested.
#[test]
fn editing_a_cell_in_a_wrapped_feed_writes_the_right_element() {
    let src = "<rss><channel><item><id>1</id></item><item><id>2</id></item></channel></rss>";
    let mut d = xml(src);
    d.set_cell(1, 0, "99").unwrap();
    assert_eq!(
        out(&d),
        "<rss><channel><item><id>1</id></item><item><id>99</id></item></channel></rss>"
    );
    assert!(d.undo());
    assert_eq!(out(&d), src);
}

/// Feeds put a newline between a tag and its CDATA. That whitespace lays
/// the file out; it is not the value, and showing it made a column of
/// links read `\r    https://…`.
#[test]
fn layout_whitespace_around_content_is_not_the_value() {
    let d = xml("<rows><row><link>\n    <![CDATA[https://example.com]]>\n  </link></row></rows>");
    assert_eq!(
        d.as_xml().unwrap().table_cell(0, 0).as_deref(),
        Some("https://example.com")
    );
}

/// Writing must land on the content, not on the whitespace beside it, and
/// must leave the layout alone.
#[test]
fn writing_lands_on_the_content_not_the_whitespace() {
    let mut d = xml("<rows><row><link>\n  <![CDATA[old]]>\n</link></row></rows>");
    d.set_cell(0, 0, "new").unwrap();
    assert_eq!(
        out(&d),
        "<rows><row><link>\n  <![CDATA[new]]>\n</link></row></rows>",
        "the value changes and the layout survives"
    );
}

/// An element that really is only whitespace keeps it: that whitespace is
/// the content, however unlikely.
#[test]
fn an_element_of_only_whitespace_keeps_it() {
    let src = "<rows><row><pad>   </pad></row></rows>";
    let d = xml(src);
    assert_eq!(d.as_xml().unwrap().table_cell(0, 0).as_deref(), Some("   "));
    assert_eq!(out(&d), src);
}

#[test]
fn text_split_around_a_comment_still_reads_whole() {
    let d = xml("<rows><row><t>one<!-- c -->two</t></row></rows>");
    assert_eq!(
        d.as_xml().unwrap().table_cell(0, 0).as_deref(),
        Some("onetwo")
    );
}

// --- Columns are fields, not positions ---
//
// Records have optional fields. An item with no `g:sale_price` used to
// shift every later value one column left, filing gtins under brands.

#[test]
fn a_missing_field_leaves_a_gap_rather_than_shifting_the_row() {
    let src = "<rows>\
        <row><a>1</a><b>2</b><c>3</c></row>\
        <row><a>4</a><c>6</c></row>\
        </rows>";
    let d = xml(src);
    let x = d.as_xml().unwrap();
    assert_eq!(x.table_headers(), vec!["a", "b", "c"]);
    assert_eq!(x.table_cell(1, 0).as_deref(), Some("4"));
    assert_eq!(
        x.table_cell(1, 1).as_deref(),
        Some(""),
        "b is absent, not shifted"
    );
    assert_eq!(x.table_cell(1, 2).as_deref(), Some("6"), "c stays under c");
}

/// Headers are the union of every row's fields, so a column that only
/// later rows have is still a column.
#[test]
fn headers_cover_fields_the_first_row_lacks() {
    let d = xml("<rows><row><a>1</a></row><row><a>2</a><z>9</z></row></rows>");
    assert_eq!(d.as_xml().unwrap().table_headers(), vec!["a", "z"]);
}

#[test]
fn editing_finds_the_right_field_when_others_are_missing() {
    let src = "<rows><row><a>1</a><b>2</b></row><row><b>3</b></row></rows>";
    let mut d = xml(src);
    // Column 1 is `b`; on row 1 that is the row's *first* child.
    d.set_cell(1, 1, "changed").unwrap();
    assert_eq!(
        out(&d),
        "<rows><row><a>1</a><b>2</b></row><row><b>changed</b></row></rows>"
    );
    assert!(d.undo());
    assert_eq!(out(&d), src);
}

/// Writing to a column the row does not have has nowhere to go, and
/// saying so beats writing it into the wrong field.
#[test]
fn editing_an_absent_field_is_refused() {
    let mut d = xml("<rows><row><a>1</a><b>2</b></row><row><a>3</a></row></rows>");
    assert!(d.set_cell(1, 1, "x").is_err());
}

/// The shape is cached, so an edit that changes it must clear the cache.
#[test]
fn adding_a_field_updates_the_columns() {
    let mut d = xml("<rows><row><a>1</a></row></rows>");
    assert_eq!(d.as_xml().unwrap().table_headers(), vec!["a"]);
    d.set_node(
        &[PathSeg::Index(0), PathSeg::Index(0), PathSeg::Text],
        vuwr_core::Node::Text("2".into()),
    )
    .unwrap();
    assert_eq!(d.as_xml().unwrap().table_cell(0, 0).as_deref(), Some("2"));
}
