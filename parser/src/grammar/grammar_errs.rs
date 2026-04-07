use ruparse::grammar::ErrorDefinition;

pub(crate) static _ERR: ErrorDefinition = ErrorDefinition {
    header: "------",
    code: "200",
    msg: "-------------",
};

pub(crate) static UNCLOSED_STRING_LIT: ErrorDefinition = ErrorDefinition {
    header: "Unclosed string",
    code: "200",
    msg: "Expected string literal to end before the end of file",
};

pub(crate) static EMPTY_CHAR_LIT: ErrorDefinition = ErrorDefinition {
    header: "Empty character",
    code: "201",
    msg: "Expected character literal to not be empty",
};

pub(crate) static MULTIPLE_TRAILING_COMMAS: ErrorDefinition = ErrorDefinition {
    header: "Multiple trailing commas",
    code: "202",
    msg: "Only one trailing comma allowed",
};

pub(crate) static EMPTY_INDEXING: ErrorDefinition = ErrorDefinition {
    header: "Empty indexing",
    code: "203",
    msg: "Expected an expression to index with",
};

pub(crate) static CHARACTER_OVERFLOW: ErrorDefinition = ErrorDefinition {
    header: "Character overflow",
    code: "204",
    msg: "Expected character literal to contain a single character",
};

pub(crate) static UNCLOSED_CHAR_LIT: ErrorDefinition = ErrorDefinition {
    header: "Unclosed character",
    code: "205",
    msg: "Unclosed character literal",
};

pub(crate) static UNKNOWN_CHARACTER: ErrorDefinition = ErrorDefinition {
    header: "Unknown character",
    code: "206",
    msg: "Unknown character literal",
};

pub(crate) static EXPECTED_UNICODE: ErrorDefinition = ErrorDefinition {
    header: "Expected Unicode",
    code: "207",
    msg: r"Expected a unicode number in unicode escape sequence. Example: \u{0b0101}",
};

pub(crate) static MISSING_ARRAY_LENGTH: ErrorDefinition = ErrorDefinition {
    header: "Missing array length",
    code: "208",
    msg: "Expected a numeric array length after semicolon",
};

pub(crate) static UNEXPECTED_ASSIGNMENT: ErrorDefinition = ErrorDefinition {
    header: "Unexpected assignment operator",
    code: "209",
    msg: "Expected a single '=' for variable initialization, found '=='",
};

pub(crate) static EMPTY_TYPE_DECLARATION: ErrorDefinition = ErrorDefinition {
    header: "Empty type declaration",
    code: "210",
    msg: "Expected a valid type after ':'",
};

pub(crate) static MISSING_STRUCT_BODY: ErrorDefinition = ErrorDefinition {
    header: "Missing struct body",
    code: "211",
    msg: "Expected a block '{ ... }' defining struct fields",
};

pub(crate) static MISSING_FUNCTION_BODY: ErrorDefinition = ErrorDefinition {
    header: "Missing function body",
    code: "212",
    msg: "Expected a code block '{ ... }' or expression '=> ...' for the function implementation",
};

pub(crate) static MISSING_RHS_EXPRESSION: ErrorDefinition = ErrorDefinition {
    header: "Missing right-hand expression",
    code: "213",
    msg: "Expected a value or expression after binary operator",
};

pub(crate) static MISSING_GENERIC_PARAMS: ErrorDefinition = ErrorDefinition {
    header: "Missing generic parameters",
    code: "214",
    msg: "Expected at least one generic parameter",
};

pub(crate) static MUTABLE_EXLUSION: ErrorDefinition = ErrorDefinition {
    header: "Mutable exclusion",
    code: "215",
    msg: "A mutable component can not be excluded",
};
