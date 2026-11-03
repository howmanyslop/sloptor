use tsrs_core::semver::version::*;

#[test]
fn decimal_components_keep_syntax_and_range_errors() {
    assert_eq!(get_uint_component("4294967295"), Ok(u32::MAX));
    assert_eq!(get_uint_component("00042"), Ok(42));
    for text in ["", "+1", "-1", "1_0", "١", "999999999999x"] {
        assert_eq!(
            get_uint_component(text),
            Err(format!("strconv.ParseUint: parsing {text:?}: invalid syntax"))
        );
    }
    assert_eq!(
        get_uint_component("4294967296"),
        Err("strconv.ParseUint: parsing \"4294967296\": value out of range".to_string())
    );
}

fn v(major: u32, minor: u32, patch: u32, prerelease: &[&str], build: &[&str]) -> Version {
    Version {
        major,
        minor,
        patch,
        prerelease: prerelease.iter().map(std::string::ToString::to_string).collect(),
        build: build.iter().map(std::string::ToString::to_string).collect(),
    }
}

#[test]
fn test_try_parse_semver() {
    let tests = [
        ("1.2.3-pre.4+build.5", v(1, 2, 3, &["pre", "4"], &["build", "5"])),
        ("1.2.3-pre.4", v(1, 2, 3, &["pre", "4"], &[])),
        ("1.2.3+build.4", v(1, 2, 3, &[], &["build", "4"])),
        ("1.2.3", v(1, 2, 3, &[], &[])),
    ];

    for (input, out) in tests {
        let got = try_parse_version(input).unwrap();
        assert_version(&got, &out);
    }
}

#[test]
fn test_version_string() {
    let tests = [
        (v(1, 2, 3, &["pre", "4"], &["build", "5"]), "1.2.3-pre.4+build.5"),
        (v(1, 2, 3, &["pre", "4"], &["build"]), "1.2.3-pre.4+build"),
        (v(1, 2, 3, &[], &["build"]), "1.2.3+build"),
        (v(1, 2, 3, &["pre", "4"], &[]), "1.2.3-pre.4"),
        (v(1, 2, 3, &[], &["build", "4"]), "1.2.3+build.4"),
        (v(1, 2, 3, &[], &[]), "1.2.3"),
    ];

    for (input, out) in tests {
        assert_eq!(input.string(), out);
        assert_eq!(input.to_string(), out);
    }
}

#[test]
fn test_version_compare() {
    let tests = [
        // https://semver.org/#spec-item-11
        // > Precedence is determined by the first difference when comparing each of these
        // > identifiers from left to right as follows: Major, minor, and patch versions are
        // > always compared numerically.
        ("1.0.0", "2.0.0", COMPARISON_LESS_THAN),
        ("1.0.0", "1.1.0", COMPARISON_LESS_THAN),
        ("1.0.0", "1.0.1", COMPARISON_LESS_THAN),
        ("2.0.0", "1.0.0", COMPARISON_GREATER_THAN),
        ("1.1.0", "1.0.0", COMPARISON_GREATER_THAN),
        ("1.0.1", "1.0.0", COMPARISON_GREATER_THAN),
        ("1.0.0", "1.0.0", COMPARISON_EQUAL_TO),
        // https://semver.org/#spec-item-11
        // > When major, minor, and patch are equal, a pre-release version has lower
        // > precedence than a normal version.
        ("1.0.0", "1.0.0-pre", COMPARISON_GREATER_THAN),
        ("1.0.1-pre", "1.0.0", COMPARISON_GREATER_THAN),
        ("1.0.0-pre", "1.0.0", COMPARISON_LESS_THAN),
        // https://semver.org/#spec-item-11
        // > identifiers consisting of only digits are compared numerically
        ("1.0.0-0", "1.0.0-1", COMPARISON_LESS_THAN),
        ("1.0.0-1", "1.0.0-0", COMPARISON_GREATER_THAN),
        ("1.0.0-2", "1.0.0-10", COMPARISON_LESS_THAN),
        ("1.0.0-10", "1.0.0-2", COMPARISON_GREATER_THAN),
        ("1.0.0-0", "1.0.0-0", COMPARISON_EQUAL_TO),
        // https://semver.org/#spec-item-11
        // > identifiers with letters or hyphens are compared lexically in ASCII sort order.
        ("1.0.0-a", "1.0.0-b", COMPARISON_LESS_THAN),
        ("1.0.0-a-2", "1.0.0-a-10", COMPARISON_GREATER_THAN),
        ("1.0.0-b", "1.0.0-a", COMPARISON_GREATER_THAN),
        ("1.0.0-a", "1.0.0-a", COMPARISON_EQUAL_TO),
        ("1.0.0-A", "1.0.0-a", COMPARISON_LESS_THAN),
        // https://semver.org/#spec-item-11
        // > Numeric identifiers always have lower precedence than non-numeric identifiers.
        ("1.0.0-0", "1.0.0-alpha", COMPARISON_LESS_THAN),
        ("1.0.0-alpha", "1.0.0-0", COMPARISON_GREATER_THAN),
        ("1.0.0-0", "1.0.0-0", COMPARISON_EQUAL_TO),
        ("1.0.0-alpha", "1.0.0-alpha", COMPARISON_EQUAL_TO),
        // https://semver.org/#spec-item-11
        // > A larger set of pre-release fields has a higher precedence than a smaller set, if all
        // > of the preceding identifiers are equal.
        ("1.0.0-alpha", "1.0.0-alpha.0", COMPARISON_LESS_THAN),
        ("1.0.0-alpha.0", "1.0.0-alpha", COMPARISON_GREATER_THAN),
        // https://semver.org/#spec-item-11
        // > Precedence for two pre-release versions with the same major, minor, and patch version
        // > MUST be determined by comparing each dot separated identifier from left to right until
        // > a difference is found [...]
        ("1.0.0-a.0.b.1", "1.0.0-a.0.b.2", COMPARISON_LESS_THAN),
        ("1.0.0-a.0.b.1", "1.0.0-b.0.a.1", COMPARISON_LESS_THAN),
        ("1.0.0-a.0.b.2", "1.0.0-a.0.b.1", COMPARISON_GREATER_THAN),
        ("1.0.0-b.0.a.1", "1.0.0-a.0.b.1", COMPARISON_GREATER_THAN),
        // https://semver.org/#spec-item-11
        // > Build metadata does not figure into precedence
        ("1.0.0+build", "1.0.0", COMPARISON_EQUAL_TO),
        ("1.0.0+build.stuff", "1.0.0", COMPARISON_EQUAL_TO),
        ("1.0.0", "1.0.0+build", COMPARISON_EQUAL_TO),
        ("1.0.0+build", "1.0.0+stuff", COMPARISON_EQUAL_TO),
        // https://semver.org/#spec-item-11
        // Edge cases for numeric and lexical comparison of prerelease identifiers.
        ("1.0.0-alpha.99999", "1.0.0-alpha.100000", COMPARISON_LESS_THAN),
        ("1.0.0-alpha.beta", "1.0.0-alpha.alpha", COMPARISON_GREATER_THAN),
    ];

    for (s1, s2, want) in tests {
        let v1 = try_parse_version(s1).unwrap_or_else(|e| panic!("{s1}: {e}"));
        let v2 = try_parse_version(s2).unwrap_or_else(|e| panic!("{s2}: {e}"));
        assert_eq!(v1.compare(&v2), want, "{s1} <=> {s2}");
    }
}

#[test]
fn test_parse_errors() {
    assert_eq!(
        try_parse_version("1.02").unwrap_err(),
        "Could not parse version string from \"1.02\""
    );
    assert_eq!(
        try_parse_version("1.2.3-").unwrap_err(),
        "Could not parse version string from \"1.2.3-\""
    );
    assert_eq!(
        try_parse_version("1.2.3-a..b").unwrap_err(),
        "Could not parse version string from \"1.2.3-a..b\""
    );
    assert_eq!(
        try_parse_version("99999999999").unwrap_err(),
        "strconv.ParseUint: parsing \"99999999999\": value out of range"
    );
    assert_eq!(try_parse_version("1.2").unwrap(), v(1, 2, 0, &[], &[]));
    assert_eq!(try_parse_version("1.2.3-\u{212A}").unwrap(), v(1, 2, 3, &["\u{212A}"], &[]));
}

fn assert_version(a: &Version, b: &Version) {
    assert_eq!(a.major, b.major);
    assert_eq!(a.minor, b.minor);
    assert_eq!(a.patch, b.patch);
    assert_eq!(a.prerelease, b.prerelease);
    assert_eq!(a.build, b.build);
}
