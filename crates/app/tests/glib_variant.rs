#![cfg(target_os = "linux")]

use glib::variant::ToVariant;

#[test]
fn variant_string_iteration_retains_valid_output_pointers() {
    let values = ["first", "second", "third"];
    let variant = values.to_variant();
    assert_eq!(
        variant
            .array_iter_str()
            .expect("strings")
            .collect::<Vec<_>>(),
        values
    );
    assert_eq!(
        variant
            .array_iter_str()
            .expect("strings")
            .rev()
            .collect::<Vec<_>>(),
        ["third", "second", "first"]
    );
    assert_eq!(
        variant.array_iter_str().expect("strings").nth(1),
        Some("second")
    );
    assert_eq!(
        variant.array_iter_str().expect("strings").last(),
        Some("third")
    );
}
