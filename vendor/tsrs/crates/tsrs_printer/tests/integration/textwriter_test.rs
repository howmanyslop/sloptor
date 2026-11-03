use tsrs_printer::EmitTextWriter;
use tsrs_printer::singlelinestringwriter::get_single_line_string_writer;
use tsrs_printer::textwriter::new_text_writer;

fn assert_reset(writer: &dyn EmitTextWriter) {
    assert_eq!(writer.string(), "");
    assert_eq!(writer.get_text_pos(), 0);
    assert_eq!(writer.get_line(), 0);
    assert_eq!(writer.get_column(), 0);
    assert_eq!(writer.get_indent(), 0);
    assert!(writer.is_at_start_of_line());
    assert!(!writer.has_trailing_comment());
    assert!(!writer.has_trailing_whitespace());
}

#[test]
fn take_output_resets_state_and_preserves_configuration() {
    for new_line in ["\n", "\r\n"] {
        let mut writer = new_text_writer(new_line, 2);
        assert_eq!(writer.take_string(), "");
        assert_reset(&*writer);

        for _ in 0..2 {
            writer.increase_indent();
            writer.write("a😀");
            assert_eq!(writer.get_column(), 5);
            writer.write_line();
            assert_eq!(writer.get_line(), 1);
            assert_eq!(writer.get_column(), 2);
            writer.write_comment("// comment ");
            assert!(writer.has_trailing_comment());
            assert!(writer.has_trailing_whitespace());
            let expected = format!("  a😀{new_line}  // comment ");
            assert_eq!(writer.get_text_pos() as usize, expected.len());
            assert_eq!(writer.string(), expected);
            assert_eq!(writer.string(), expected);
            assert_eq!(writer.take_string(), expected);
            assert_reset(&*writer);
            assert_eq!(writer.take_string(), "");
        }
    }
}

#[test]
fn indentation_handles_empty_writes_and_deep_levels() {
    let mut writer = new_text_writer("\n", 0);
    writer.write("");
    assert_reset(&*writer);
    for _ in 0..32 {
        writer.increase_indent();
    }
    writer.write("x");
    writer.write("y");
    assert_eq!(writer.get_column(), 130);
    assert_eq!(writer.take_string(), format!("{}xy", " ".repeat(128)));
    assert_reset(&*writer);
    writer.write("z");
    assert_eq!(writer.take_string(), "z");
}

#[test]
fn default_take_output_resets_single_line_writer() {
    let (mut writer, release) = get_single_line_string_writer();
    for _ in 0..2 {
        writer.write("a");
        writer.write_line();
        assert!(writer.has_trailing_whitespace());
        assert_eq!(writer.take_string(), "a ");
        assert_eq!(writer.get_text_pos(), 0);
        assert!(!writer.has_trailing_whitespace());
        assert_eq!(writer.take_string(), "");
    }
    release();
}
