//! Property tests: generated documents must be stable under
//! parse(serialize(doc)) — i.e. serialization reaches a fixpoint after one
//! round trip, so real files never drift no matter what was edited in.

use proptest::prelude::*;
use vuwr_core::{Cell, CsvDoc, Document, FormatHint, LineEnding};

fn cell_strategy() -> impl Strategy<Value = Cell> {
    // Alphabet heavy on the troublemakers: the delimiters, quote, CR/LF,
    // spaces, and a multi-byte character.
    let value = prop::string::string_regex("[a-z0-9,;\t\"\\r\\n é]{0,6}").expect("valid regex");
    (value, prop::bool::ANY).prop_map(|(value, quoted)| Cell { value, quoted })
}

fn doc_strategy() -> impl Strategy<Value = (CsvDoc, FormatHint)> {
    prop_oneof![
        Just((b',', FormatHint::Csv)),
        Just((b'\t', FormatHint::Tsv))
    ]
    .prop_flat_map(|(delimiter, hint)| {
        let row = prop::collection::vec(cell_strategy(), 0..6);
        let rows = prop::collection::vec(row, 0..8);
        (
            Just(delimiter),
            Just(hint),
            prop_oneof![Just(LineEnding::Lf), Just(LineEnding::CrLf)],
            prop::bool::ANY,
            rows,
        )
            .prop_map(|(delimiter, hint, line_ending, trailing_newline, rows)| {
                (
                    CsvDoc::from_parts(delimiter, line_ending, trailing_newline, rows),
                    hint,
                )
            })
    })
}

proptest! {
    #[test]
    fn serialize_parse_serialize_is_stable((csv, hint) in doc_strategy()) {
        let bytes = csv.serialize();
        let doc = Document::parse(&bytes, hint).expect("serialized output must parse");
        prop_assert_eq!(doc.serialize(), bytes);
    }
}
