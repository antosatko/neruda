use ruparse::Parser;
use ruparse::api::ext::*;
use ruparse::grammar::validator::Validator;
use ruparse::grammar::{MatchToken, VarKind, VariableKind};
pub use ruparse::lexer::Token;
use ruparse::lexer::{ControlTokenKind, PreprocessorError, TokenKinds};

const IDENTIFIER: VarKind<'static> = local("identifier");
const IDENTIFIER_VAR: (&'static str, VariableKind) = ("identifier", VariableKind::Node);

const KEYWORDS: &[&'static str] = &[
    "system",
    "invoke",
    "init",
    "struct",
    "component",
    "resource",
    "var",
    "function",
    "return",
    "if",
    "else",
    "loop",
    "while",
    "break",
    "continue",
    "as",
    "mut",
    "type",
    "before",
    "after",
    "foreign",
    "enum",
    "const",
    "trait",
    "impl",
    "for",
    "using",
    "pub",
];

const KEYWORDS_NON_BLOCKING: &[&'static str] = &["where", "on", "import"];

mod grammar_errs;

fn keyword(kw: &'static str) -> MatchToken<'static> {
    assert!(
        KEYWORDS.contains(&kw) || KEYWORDS_NON_BLOCKING.contains(&kw),
        "{kw} must be a keyword"
    );
    word(kw)
}

fn is_numeric(str: &str) -> bool {
    str.chars()
        .nth(0)
        .expect("Expected a non empty text")
        .is_numeric()
}

fn count_hashes(tokens: &[Token]) -> usize {
    tokens
        .iter()
        .take_while(|t| t.kind == TokenKinds::Token("#"))
        .count()
}

pub fn gen_parser<'src>() -> Parser<'static> {
    let mut parser = Parser::new();

    parser.lexer.add_tokens(
        "+ - * / % \\ ; \" ' : :: ( { [ < > ] } ) | & ! ? = . , # == != += -= *= /= %= && || >= <= =>"
            .split_whitespace(),
    );
    parser.grammar.ignored.push(TokenKinds::Complex("comment"));
    parser.lexer.preprocessors.push(|src, tokens| {
        use ruparse::lexer::TokenKinds;
        let mut i = 0;
        let mut result = Vec::new();

        while i < tokens.len() {
            let tok = &tokens[i];
            match &tok.kind {
                TokenKinds::Text => {
                    let numeric = is_numeric(tok.stringify(src));
                    if !numeric {
                        result.push(*tok);
                        i += 1;
                        continue;
                    }
                    if let Some(t) = tokens.get(i + 1)
                        && t.kind == TokenKinds::Token(".")
                    {
                        if let Some(t) = tokens.get(i + 2)
                            && t.kind == TokenKinds::Text
                            && is_numeric(t.stringify(src))
                        {
                            let num = Token {
                                index: tok.index,
                                len: tok.len + 1 + t.len,
                                location: tok.location,
                                kind: TokenKinds::Complex("float"),
                            };
                            result.push(num);
                            i += 3;
                            continue;
                        }
                    }
                    let num = Token {
                        index: tok.index,
                        len: tok.len,
                        location: tok.location,
                        kind: TokenKinds::Complex("numeric"),
                    };
                    result.push(num);
                }
                _ => result.push(*tok),
            }
            i += 1;
        }

        Ok(result)
    });

    parser.lexer.preprocessors.push(|src, tokens| {
        use ruparse::lexer::TokenKinds;
        let mut i = 0;
        let mut result = Vec::new();

        'main: while i < tokens.len() {
            let tok = &tokens[i];
            match &tok.kind {
                // might be a comment
                TokenKinds::Token(t) if *t == "/" => {
                    let start = i;
                    if let Some(t) = tokens.get(i + 1)
                        && t.kind == TokenKinds::Token("/")
                    {
                        if let Some(t) = tokens.get(i + 2)
                            && t.kind == TokenKinds::Token("/")
                        {
                            // is a documentation comment
                            while let Some(t) = tokens.get(i)
                                && t.kind != TokenKinds::Control(ControlTokenKind::Eol)
                            {
                                i += 1;
                            }
                            let tok = Token {
                                index: tokens[start].index,
                                len: tokens[i].index - tokens[start].index,
                                location: tokens[start].location,
                                kind: TokenKinds::Complex("docstr"),
                            };
                            result.push(tok);
                            continue;
                        }
                        if let Some(t) = tokens.get(i + 2)
                            && t.kind == TokenKinds::Token("!")
                        {
                            // is a top level documentation comment
                            while let Some(t) = tokens.get(i)
                                && t.kind != TokenKinds::Control(ControlTokenKind::Eol)
                            {
                                i += 1;
                            }
                            let tok = Token {
                                index: tokens[start].index,
                                len: tokens[i].index - tokens[start].index,
                                location: tokens[start].location,
                                kind: TokenKinds::Complex("tl docstr"),
                            };
                            result.push(tok);
                            continue;
                        } else {
                            // is a comment
                            while let Some(t) = tokens.get(i)
                                && t.kind != TokenKinds::Control(ControlTokenKind::Eol)
                            {
                                i += 1;
                            }
                            let tok = Token {
                                index: tokens[start].index,
                                len: tokens[i].index - tokens[start].index,
                                location: tokens[start].location,
                                kind: TokenKinds::Complex("comment"),
                            };
                            result.push(tok);
                            continue;
                        }
                    }
                    result.push(tokens[i]);
                }
                TokenKinds::Token(t) if *t == "'" => match tokens.get(i + 1).map(|t| &t.kind) {
                    Some(TokenKinds::Token(t)) if *t == "\\" => {
                        match tokens.get(i + 2).map(|t| t) {
                            Some(t) => {
                                let str = t.stringify(src);
                                let literal = if "\"\\\'0abfnrtv".contains(str) {
                                    // classic escapes
                                    let tok = Token {
                                        index: tok.index,
                                        len: tokens[i + 2].len + 3,
                                        location: tok.location,
                                        kind: TokenKinds::Complex("char"),
                                    };
                                    i += 3;
                                    tok
                                } else if str == "u" {
                                    // unicode
                                    match (
                                        tokens.get(i + 3).map(|t| &t.kind),
                                        tokens.get(i + 4).map(|t| &t.kind),
                                        tokens.get(i + 5).map(|t| &t.kind),
                                    ) {
                                        (
                                            Some(TokenKinds::Token(open)),
                                            Some(TokenKinds::Complex(code)),
                                            Some(TokenKinds::Token(close)),
                                        ) if *open == "{"
                                            && *code == "numeric"
                                            && *close == "}" =>
                                        {
                                            let tok = Token {
                                                index: tok.index,
                                                len: tokens[i + 4].len + 6,
                                                location: tok.location,
                                                kind: TokenKinds::Complex("char"),
                                            };
                                            i += 6;
                                            tok
                                        }
                                        (Some(TokenKinds::Token(op)), Some(_), _) if *op == "{" => {
                                            Err(PreprocessorError {
                                                err: grammar_errs::EXPECTED_UNICODE,
                                                location: tokens[i + 4].location,
                                                len: tokens[i + 4].len,
                                            })?
                                        }
                                        _ => Err(PreprocessorError {
                                            err: grammar_errs::EXPECTED_UNICODE,
                                            location: tokens[i + 2].location,
                                            len: tokens[i + 2].len,
                                        })?,
                                    }
                                } else {
                                    // unknown
                                    Err(PreprocessorError {
                                        err: grammar_errs::UNKNOWN_CHARACTER,
                                        location: tokens[i + 1].location,
                                        len: t.len + 1,
                                    })?
                                };

                                match tokens.get(i).map(|t| &t.kind) {
                                    Some(TokenKinds::Token(t)) if *t == "'" => {
                                        result.push(literal);
                                        i += 1;
                                        continue;
                                    }
                                    _ => Err(PreprocessorError {
                                        err: grammar_errs::UNCLOSED_CHAR_LIT,
                                        location: tok.location,
                                        len: t.len + 2,
                                    })?,
                                }
                            }
                            _ => Err(PreprocessorError {
                                err: grammar_errs::UNCLOSED_CHAR_LIT,
                                location: tok.location,
                                len: tok.len + tokens[i + 1].len,
                            })?,
                        }
                    }
                    Some(TokenKinds::Text) => {
                        let chars = tokens[i + 1].stringify(src).chars();
                        if chars.count() > 1 {
                            Err(PreprocessorError {
                                err: grammar_errs::CHARACTER_OVERFLOW,
                                location: tok.location,
                                len: tok.len + tokens[i + 1].len,
                            })?
                        }
                        match tokens.get(i + 2).map(|t| &t.kind) {
                            Some(TokenKinds::Token(t)) if *t == "'" => {
                                let tok = Token {
                                    index: tok.index,
                                    len: tokens[i + 1].len + 2,
                                    location: tok.location,
                                    kind: TokenKinds::Complex("char"),
                                };
                                result.push(tok);
                                i += 3;
                                continue;
                            }
                            _ => Err(PreprocessorError {
                                err: grammar_errs::UNCLOSED_CHAR_LIT,
                                location: tok.location,
                                len: tok.len + tokens[i + 1].len,
                            })?,
                        }
                    }
                    Some(TokenKinds::Token(t)) if *t == "'" => Err(PreprocessorError {
                        err: grammar_errs::EMPTY_CHAR_LIT,
                        location: tok.location,
                        len: 1,
                    })?,
                    Some(_) => Err(PreprocessorError {
                        err: grammar_errs::UNKNOWN_CHARACTER,
                        location: tokens[i + 1].location,
                        len: tokens[i + 1].len,
                    })?,
                    None => Err(PreprocessorError {
                        err: grammar_errs::UNCLOSED_CHAR_LIT,
                        location: tok.location,
                        len: 1,
                    })?,
                },
                TokenKinds::Token(t) if *t == "\"" => {
                    let mut offset = 1;
                    while let Some(token) = tokens.get(i + offset) {
                        match &token.kind {
                            TokenKinds::Token(t) if *t == "\"" => {
                                let token = Token {
                                    index: tok.index,
                                    len: token.index - tok.index + 1,
                                    location: tok.location,
                                    kind: TokenKinds::Complex("string"),
                                };
                                result.push(token);
                                i += offset + 1;
                                continue 'main;
                            }
                            TokenKinds::Token(t) if *t == "\\" => offset += 2,
                            _ => offset += 1,
                        }
                    }
                    Err(PreprocessorError {
                        err: grammar_errs::UNCLOSED_STRING_LIT,
                        location: tok.location,
                        len: tok.len,
                    })?
                }
                TokenKinds::Token(t) if *t == "#" => {
                    let count = count_hashes(&tokens[i..]);
                    if let Some(t) = tokens.get(i + count)
                        && t.kind == TokenKinds::Token("\"")
                    {
                        let mut offset = count + 1;
                        while let Some(token) = tokens.get(i + offset) {
                            match &token.kind {
                                TokenKinds::Token(t) if *t == "\"" => {
                                    if count_hashes(&tokens[i + offset + 1..]) < count {
                                        i += 1;
                                        continue;
                                    }
                                    offset += count;
                                    let token = Token {
                                        index: tok.index,
                                        len: tokens[i + offset].index - tok.index + 1,
                                        location: tok.location,
                                        kind: TokenKinds::Complex("string"),
                                    };
                                    result.push(token);
                                    i += offset + 1;
                                    continue 'main;
                                }
                                TokenKinds::Token(t) if *t == "\\" => offset += 2,
                                _ => offset += 1,
                            }
                        }
                        Err(PreprocessorError {
                            err: grammar_errs::UNCLOSED_STRING_LIT,
                            location: tok.location,
                            len: tok.len,
                        })?
                    }
                    result.push(*tok);
                }
                // TokenKinds::Whitespace => (),
                _ => result.push(*tok),
            }
            i += 1;
        }

        Ok(result)
    });

    let keywords = parser
        .grammar
        .new_enum("keyword")
        .options(KEYWORDS.iter().map(|kw| word(kw)))
        .build();

    let operators = parser
        .grammar
        .new_enum("operator")
        .options(
            "+ - * / = % == != < > <= >= || && += -= *= /= %="
                .split_whitespace()
                .map(|t| token(t)),
        )
        .build();

    let delimiters_peek_enum = parser
        .grammar
        .new_enum("delimiters")
        .options(") ] }".split_whitespace().map(|c| token(c)))
        .options([eof(), newline(), token(",")])
        .build();

    let delimiters_peek = parser
        .grammar
        .new_node("delimiter")
        .rules([peek(delimiters_peek_enum)])
        .build();

    let delimiters_consume = parser
        .grammar
        .new_enum("terminator")
        .options([token(";")])
        .build();

    let access_modifier = parser
        .grammar
        .new_node("access modifier")
        .rules([maybe(keyword("pub"))
            .set("public")
            .then([maybe(token("(")).then([
                is_one_of([
                    option(word("mod")).set("modifier"),
                    option(word("project")).set("modifier"),
                ]),
                is(token(")")),
            ])])])
        .variables([node_var("public"), node_var("modifier")])
        .build();

    let docstr = parser
        .grammar
        .new_node("doc string")
        .rules([while_(complex("docstr")).set("docstr")])
        .variables([list_var("docstr")])
        .build();

    let tl_docstr = parser
        .grammar
        .new_node("top level doc string")
        .rules([while_(complex("tl docstr")).set("docstr")])
        .variables([list_var("docstr")])
        .build();

    let ident = parser
        .grammar
        .new_node("identifier")
        .rules([
            isnt(keywords)
                .hint("Keywords are reserved and can not be used for identifiers")
                .important(),
            is(text()).set(IDENTIFIER),
        ])
        .variables([node_var("identifier")])
        .build();

    let ident_path = parser
        .grammar
        .new_node("identifier path")
        .rules([
            is(ident).set("path").commit(),
            while_(token("::")).then([is(ident).set("path")]),
        ])
        .variables([list_var("path")])
        .build();

    let alias = parser
        .grammar
        .new_node("alias")
        .rules([is(keyword("as")).commit(), is(ident).set(IDENTIFIER)])
        .variables([IDENTIFIER_VAR])
        .build();

    let array_literal = parser
        .grammar
        .new_node("array literal")
        .rules([
            is(token("[")).commit(),
            loop_().then([
                maybe(node("expression")).set("elements"),
                is_one_of([
                    option(token(","))
                        .then([maybe(token(",")).fail(&grammar_errs::MULTIPLE_TRAILING_COMMAS)]),
                    option(token("]")).return_node(),
                ]),
            ]),
        ])
        .variables([list_var("elements")])
        .build();

    let tuple_literal = parser
        .grammar
        .new_node("tuple literal")
        .rules([
            is(token("(")).commit(),
            loop_().then([
                maybe(node("expression")).set("expressions"),
                is_one_of([
                    option(token(","))
                        .then([maybe(token(",")).fail(&grammar_errs::MULTIPLE_TRAILING_COMMAS)]),
                    option(token(")")).return_node(),
                ]),
            ]),
        ])
        .variables([list_var("expressions")])
        .build();

    let named_argument = parser
        .grammar
        .new_node("named parameter")
        .rules([
            is(ident).set(IDENTIFIER).commit(),
            is(token(":")),
            is(node("expression")).set("expression"),
        ])
        .variables([IDENTIFIER_VAR, node_var("expression")])
        .build();

    let ident_path_literal = parser
        .grammar
        .new_node("identifier path literal")
        .rules([
            is(ident_path).set(IDENTIFIER).commit(),
            maybe(node("generics")).set("generics"),
        ])
        .variables([IDENTIFIER_VAR, node_var("generics")])
        .build();

    let struct_literal = parser
        .grammar
        .new_node("struct literal")
        .rules([
            is(keyword("struct")).commit(),
            maybe(ident_path_literal).set("type"),
            is(token("{")).commit(),
            loop_().then([is_one_of([
                option(named_argument).set("arguments"),
                option(token("}")).return_node(),
            ])]),
        ])
        .variables([list_var("arguments"), node_var("type")])
        .build();

    let literals = parser
        .grammar
        .new_enum("literal")
        .options([
            ident_path_literal,
            complex("string"),
            struct_literal,
            complex("char"),
            array_literal,
            tuple_literal,
            complex("numeric"),
            complex("float"),
        ])
        .build();

    let field = parser
        .grammar
        .new_node("field access")
        .rules([is(token(".")), is(ident).commit().set("identifier")])
        .variables([node_var("identifier")])
        .build();

    let call = parser
        .grammar
        .new_node("function call")
        .rules([
            is(token("(")).commit(),
            loop_().then([
                maybe(node("expression")).set("expressions"),
                is_one_of([
                    option(token(","))
                        .then([maybe(token(",")).fail(&grammar_errs::MULTIPLE_TRAILING_COMMAS)]),
                    option(token(")")).return_node(),
                ]),
            ]),
        ])
        .variables([list_var("expressions")])
        .build();

    let indexing = parser
        .grammar
        .new_node("indexing")
        .rules([
            is(token("[")).commit(),
            is_one_of([
                option(token("]")).fail(&grammar_errs::EMPTY_INDEXING),
                option(node("expression")).set("index"),
            ])
            .hint("Must index with a valid expression"),
            is(token("]")),
        ])
        .variables([node_var("index")])
        .build();

    let ref_tokens = parser
        .grammar
        .new_enum("ref token")
        .options([token("&"), token("*")])
        .build();

    let refs = parser
        .grammar
        .new_node("ref cast")
        .rules([is(token(".")), is(ref_tokens).set("ref token").commit()])
        .variables([node_var("ref token")])
        .build();

    let value_tails = parser
        .grammar
        .new_enum("value tail")
        .options([field, call, indexing, refs])
        .build();

    let unary_operators = parser
        .grammar
        .new_enum("unary operator")
        .options([token("-"), token("!")])
        .build();

    let end_stmt = parser
        .grammar
        .new_node("end statement")
        .rules([is_one_of([
            option(delimiters_peek),
            option(delimiters_consume).set("consumed"),
        ])
        .hint("Expected to end statement on a delimiter")])
        .variables([node_var("consumed")])
        .build();

    let import = parser
        .grammar
        .new_node("import")
        .has(access_modifier, "access mod")
        .rules([
            is(keyword("import")).commit(),
            is(ident_path).set(IDENTIFIER).set(global("imports")),
            maybe(alias).set("alias"),
            is(end_stmt),
        ])
        .variables([IDENTIFIER_VAR, node_var("alias")])
        .build();

    let path_set_selector = parser
        .grammar
        .new_node("path set")
        .rules([
            is(token("{")).commit(),
            is(node("path selector")).set("selectors"),
            while_(end_stmt).then([is_one_of([
                option(node("path selector")).set("selectors"),
                option(token("}")).return_node(),
            ])
            .hint("do you see?")]),
        ])
        .variables([list_var("selectors")])
        .build();

    let star = parser
        .grammar
        .new_node("star")
        .rules([is(token("*"))])
        .build();

    let path_endings = parser
        .grammar
        .new_enum("path endings")
        .options([star, path_set_selector])
        .build();

    let many_path_selector = parser
        .grammar
        .new_node("path selector")
        .rules([
            is(ident).set("path").commit(),
            while_(token("::")).then([
                maybe(path_endings).set("ends on").return_node(),
                is(ident)
                    .set("path")
                    .then([maybe(alias).set("ends on").return_node()]),
            ]),
        ])
        .variables([list_var("path"), node_var("ends on")])
        .build();

    let using = parser
        .grammar
        .new_node("using")
        .rules([
            is(keyword("using")).commit(),
            is(many_path_selector).set("selector"),
            is(end_stmt),
        ])
        .variables([node_var("selector")])
        .build();

    let value = parser
        .grammar
        .new_node("value")
        .rules([
            while_(unary_operators).set("unary operators"),
            is(literals).commit().set("literal"),
            while_(value_tails)
                .set("tail")
                .then([maybe(end_stmt).return_node()]),
        ])
        .variables([
            node_var("literal"),
            list_var("tail"),
            list_var("unary operators"),
        ])
        .build();

    let expression = parser
        .grammar
        .new_node("expression")
        .rules([
            is(value).set("lvalue").commit(),
            while_(operators).set("rest").then([is_one_of([
                option(value).set("rest"),
                // If 'value' fails to match after an operator, trigger the custom error
                option(any()).fail(&grammar_errs::MISSING_RHS_EXPRESSION),
            ])]),
        ])
        .variables([node_var("lvalue"), list_var("rest")])
        .build();

    let generic_parameter = parser
        .grammar
        .new_node("generic parameter")
        .rules([
            is(ident).set(IDENTIFIER).commit(),
            maybe(token(":")).then([
                is(ident_path).set("constraints"),
                while_(token("+")).then([is(ident_path).set("constraints")]),
            ]),
        ])
        .variables([IDENTIFIER_VAR, list_var("constraints")])
        .build();

    let generic_params = parser
        .grammar
        .new_node("geneic parameters")
        .rules([
            is(token("<")).commit(),
            loop_().then([
                maybe(token(">")).fail(&grammar_errs::MISSING_GENERIC_PARAMS),
                is(generic_parameter).set("parameters"),
                is_one_of([
                    option(token(","))
                        .then([maybe(token(",")).fail(&grammar_errs::MULTIPLE_TRAILING_COMMAS)]),
                    option(token(">")).return_node(),
                ]),
            ]),
        ])
        .variables([list_var("parameters")])
        .build();

    let struct_type_literal = parser
        .grammar
        .new_node("struct type literal")
        .rules([
            is(keyword("struct")).commit(),
            is_one_of([
                option(token("{")),
                option(any()).fail(&grammar_errs::MISSING_STRUCT_BODY),
            ]),
            loop_().then([is_one_of([
                option(node("parameter")).set("parameters"),
                option(token("}")).return_node(),
            ])]),
        ])
        .variables([list_var("parameters")])
        .build();

    let enum_type_literal_variant = parser
        .grammar
        .new_node("enum type varaint")
        .has(docstr, "docs")
        .rules([
            is(ident).set(IDENTIFIER).commit(),
            maybe(token("=")).then([is_one_of([
                option(expression).set("expression"),
                option(any()).fail(&grammar_errs::MISSING_EXPRESSION),
            ])]),
            is(end_stmt),
        ])
        .variables([IDENTIFIER_VAR, node_var("expression")])
        .build();

    let enum_type_literal = parser
        .grammar
        .new_node("enum type literal")
        .rules([
            is(keyword("enum")).commit(),
            maybe(token("(")).then([is(expression).set("step"), is(token(")"))]),
            maybe(token(":")).then([is(node("type")).set("representation")]),
            is_one_of([
                option(token("{")),
                option(any()).fail(&grammar_errs::MISSING_ENUM_BODY),
            ]),
            loop_().then([is_one_of([
                option(enum_type_literal_variant).set("variants"),
                option(token("}")).return_node(),
            ])]),
        ])
        .variables([
            list_var("variants"),
            node_var("representation"),
            node_var("step"),
        ])
        .build();

    let array_type_literal = parser
        .grammar
        .new_node("array type literal")
        .rules([
            is(token("[")).commit(),
            is(node("type")).set("type"),
            maybe(token(";")).then([is_one_of([
                option(token("]")).fail(&grammar_errs::MISSING_ARRAY_LENGTH),
                option(expression).set("length"),
            ])]),
            is(token("]")),
        ])
        .variables([node_var("type"), node_var("length")])
        .build();

    let tuple_type_literal = parser
        .grammar
        .new_node("tuple type literal")
        .rules([
            is(token("(")).commit(),
            loop_().then([
                maybe(node("type")).set("parameters"),
                is_one_of([
                    option(token(","))
                        .then([maybe(token(",")).fail(&grammar_errs::MULTIPLE_TRAILING_COMMAS)]),
                    option(token(")")).return_node(),
                ]),
            ]),
        ])
        .variables([list_var("parameters")])
        .build();

    let generic_impl = parser
        .grammar
        .new_node("generics")
        .rules([
            is(token("<")).commit(),
            loop_().then([
                maybe(node("type")).set("parameters"),
                is_one_of([
                    option(token(","))
                        .then([maybe(token(",")).fail(&grammar_errs::MULTIPLE_TRAILING_COMMAS)]),
                    option(token(">")).return_node(),
                ]),
            ]),
        ])
        .variables([list_var("parameters")])
        .build();

    let type_path = parser
        .grammar
        .new_node("type path")
        .rules([
            is(ident_path).commit().set(IDENTIFIER),
            maybe(generic_impl).set("generics"),
        ])
        .variables([IDENTIFIER_VAR, node_var("generics")])
        .build();

    let type_literal = parser
        .grammar
        .new_enum("type literal")
        .options([
            type_path,
            struct_type_literal,
            array_type_literal,
            tuple_type_literal,
            enum_type_literal,
        ])
        .build();

    let type_prefix_variants = parser
        .grammar
        .new_enum("type prefix variants")
        .options([token("&"), token("&&")])
        .build();

    let type_prefix = parser
        .grammar
        .new_node("type prefix")
        .rules([while_(type_prefix_variants).set("prefix")])
        .variables([list_var("prefix")])
        .build();

    let type_ = parser
        .grammar
        .new_node("type")
        .rules([
            maybe(type_prefix).set("prefix"),
            is(type_literal).commit().set("literal"),
        ])
        .variables([node_var("literal"), node_var("prefix")])
        .build();

    let label = parser
        .grammar
        .new_node("label")
        .rules([is(ident).set(IDENTIFIER).start(), is(token(":"))])
        .variables([IDENTIFIER_VAR])
        .build();

    let parameter = parser
        .grammar
        .new_node("parameter")
        .has(docstr, "docs")
        .rules([
            is(ident).set("identifier").commit(),
            is(token(":")),
            is_one_of([
                option(token(",")).fail(&grammar_errs::EMPTY_TYPE_DECLARATION),
                option(token(")")).fail(&grammar_errs::EMPTY_TYPE_DECLARATION),
                option(type_).set("type"),
            ]),
            maybe(token("=")).then([is(expression).set("default value")]),
        ])
        .variables([
            node_var("identifier"),
            node_var("type"),
            node_var("default value"),
        ])
        .build();

    let parameter_list = parser
        .grammar
        .new_node("parameters")
        .rules([
            is(token("(")).commit(),
            loop_().then([
                maybe(parameter).set("parameters"),
                is_one_of([
                    option(token(","))
                        .then([maybe(token(",")).fail(&grammar_errs::MULTIPLE_TRAILING_COMMAS)]),
                    option(token(")")).return_node(),
                ]),
            ]),
        ])
        .variables([list_var("parameters")])
        .build();

    let const_kw = parser
        .grammar
        .new_node("const")
        .has(docstr, "docs")
        .has(access_modifier, "access mod")
        .rules([
            is(keyword("const")).commit(),
            is(ident).set(IDENTIFIER),
            is(token(":")).hint("Constants must specify the type"),
            is(type_).set("type"),
            is(token("=")).hint("Constants must be initialized on declaration"),
            is(expression).set("expression"),
            is(end_stmt),
        ])
        .variables([IDENTIFIER_VAR, node_var("type"), node_var("expression")])
        .build();

    let expression_st = parser
        .grammar
        .new_node("expression statement")
        .rules([is(expression).set("expression").commit(), is(end_stmt)])
        .variables([node_var("expression")])
        .build();

    let variable_st = parser
        .grammar
        .new_node("variable")
        .rules([
            is(keyword("var")).commit(),
            is(ident).set(IDENTIFIER),
            maybe(token(":")).then([is(type_).set("type").important()]),
            maybe_one_of([
                option(token("==")).fail(&grammar_errs::UNEXPECTED_ASSIGNMENT),
                option(token("=")).then([is(expression)
                    .set("expression")
                    .hint("Variable must be initialized to a valid expression")
                    .important()]),
            ]),
            is(end_stmt),
        ])
        .variables([IDENTIFIER_VAR, node_var("type"), node_var("expression")])
        .build();

    let return_st = parser
        .grammar
        .new_node("return")
        .rules([
            is(keyword("return")).commit(),
            maybe(expression).set("expression"),
            is(end_stmt),
        ])
        .variables([node_var("expression")])
        .build();

    let continue_st = parser
        .grammar
        .new_node("continue")
        .maybe_has(label, "label")
        .rules([is(keyword("continue")).start()])
        .build();

    let break_st = parser
        .grammar
        .new_node("break")
        .maybe_has(label, "label")
        .rules([is(keyword("break")).start()])
        .build();

    let code_block = parser
        .grammar
        .new_node("code block")
        .rules([
            is(token("{")).commit(),
            loop_().then([is_one_of([
                option(enumerator("statement")).set("statements"),
                option(token("}")).return_node(),
            ])
            .hint("Potentially unclosed code body")]),
        ])
        .variables([list_var("statements")])
        .build();

    let code_expression = parser
        .grammar
        .new_node("code statement")
        .rules([
            is(token("=>")).commit(),
            is(expression_st).set("expression"),
        ])
        .variables([node_var("expression")])
        .build();

    let code_body = parser
        .grammar
        .new_enum("code body")
        .options([code_block, code_expression])
        .build();

    let loop_st = parser
        .grammar
        .new_node("loop")
        .maybe_has(label, "label")
        .rules([
            is(keyword("loop")).commit().start(),
            is(code_body).set("code body"),
        ])
        .variables([node_var("code body")])
        .build();

    let else_if_st = parser
        .grammar
        .new_node("else if")
        .rules([
            is(keyword("else")),
            is(keyword("if")).commit(),
            is(expression).set("expression"),
            is(code_body).set("code body"),
        ])
        .variables([node_var("expression"), node_var("code body")])
        .build();

    let else_st = parser
        .grammar
        .new_node("else")
        .rules([is(keyword("else")).commit(), is(code_body).set("code body")])
        .variables([node_var("code body")])
        .build();

    let if_st = parser
        .grammar
        .new_node("if")
        .rules([
            is(keyword("if")).commit(),
            is(expression).set("expression"),
            is(code_body).set("code body"),
            while_(else_if_st).set("else if"),
            maybe(else_st).set("else"),
        ])
        .variables([
            node_var("expression"),
            node_var("code body"),
            list_var("else if"),
            node_var("else"),
        ])
        .build();

    let while_st = parser
        .grammar
        .new_node("while")
        .maybe_has(label, "label")
        .rules([
            is(keyword("while")).commit().start(),
            is(expression).set("expression"),
            is(code_body).set("code body"),
        ])
        .variables([node_var("expression"), node_var("code body")])
        .build();

    let invocation = parser
        .grammar
        .new_node("invocation")
        .rules([
            is(ident_path_literal).set(IDENTIFIER).commit(),
            is(call).set("arguments"),
            is(end_stmt),
        ])
        .variables([node_var("arguments"), IDENTIFIER_VAR])
        .build();

    let invoke_st = parser
        .grammar
        .new_node("invoke")
        .rules([
            is(keyword("invoke")).commit(),
            is(token("{")),
            loop_().then([is_one_of([
                option(invocation).set("invocations"),
                option(token("}")).return_node(),
            ])
            .hint("Potentially unclosed code body")]),
        ])
        .variables([list_var("invocations")])
        .build();

    // references
    let _statements = parser
        .grammar
        .new_enum("statement")
        .options([
            variable_st,
            return_st,
            continue_st,
            break_st,
            loop_st,
            if_st,
            while_st,
            invoke_st,
            expression_st,
        ])
        .build();

    let function = parser
        .grammar
        .new_node("function")
        .has(docstr, "docs")
        .has(access_modifier, "access mod")
        .rules([
            maybe(keyword("invoke")).set("invoke"),
            is(keyword("function")).commit().start(),
            is(ident).set(IDENTIFIER),
            maybe(generic_params).set("generic parameters"),
            is(parameter_list).set("parameters"),
            maybe(token(":")).then([is(type_).set("return type").important()]),
            is_one_of([
                option(code_body).set("code body"),
                option(any()).fail(&grammar_errs::MISSING_FUNCTION_BODY),
            ]),
        ])
        .variables([
            IDENTIFIER_VAR,
            node_var("generic parameters"),
            node_var("parameters"),
            node_var("return type"),
            node_var("code body"),
            node_var("invoke"),
        ])
        .build();

    let trait_kw = parser
        .grammar
        .new_node("trait")
        .has(docstr, "docs")
        .has(access_modifier, "access mod")
        .rules([
            is(keyword("trait")).commit(),
            is(ident).set(IDENTIFIER),
            is(token("{")),
            loop_().then([is_one_of([
                option(function).set("methods"),
                option(token("}")).return_node(),
            ])]),
        ])
        .variables([IDENTIFIER_VAR, list_var("methods")])
        .build();

    let type_impl = parser
        .grammar
        .new_node("type implementation")
        .rules([is(any()).important(), is(type_).set("type").commit()])
        .variables([node_var("type")])
        .build();

    let trait_impl = parser
        .grammar
        .new_node("trait implementation")
        .rules([
            is(ident_path).set("trait"),
            is(keyword("for")).commit().set("kw"),
            is(type_).set("type"),
        ])
        .variables([node_var("trait"), node_var("type"), node_var("kw")])
        .build();

    let impl_variants = parser
        .grammar
        .new_enum("impl type")
        .options([trait_impl, type_impl])
        .build();

    let impl_kw = parser
        .grammar
        .new_node("impl")
        .rules([
            is(keyword("impl")).commit(),
            maybe(generic_params).set("generic parameters"),
            is(impl_variants).set("type"),
            is(token("{")),
            loop_().then([is_one_of([
                option(function).set("methods"),
                option(token("}")).return_node(),
            ])]),
        ])
        .variables([
            node_var("type"),
            node_var("generic parameters"),
            list_var("methods"),
        ])
        .build();

    let clause_select_component = parser
        .grammar
        .new_node("selection component")
        .rules([
            maybe(keyword("mut"))
                .set("mutable")
                .then([maybe_one_of([
                    option(token("?")).set("modifier"),
                    option(token("!")).fail(&grammar_errs::MUTABLE_EXLUSION),
                ])])
                .otherwise([maybe_one_of([
                    option(token("?")).set("modifier"),
                    option(token("!")).set("modifier"),
                ])]),
            is(ident_path).set("component").commit(),
            maybe(alias).set("alias"),
        ])
        .variables([
            node_var("mutable"),
            node_var("modifier"),
            node_var("component"),
            node_var("alias"),
        ])
        .build();

    let clause_select = parser
        .grammar
        .new_node("select")
        .has(docstr, "docs")
        .rules([
            maybe(keyword("foreign")).set("foreign"),
            is(ident).set(IDENTIFIER).commit(),
            is(token(":")),
            is(clause_select_component).set("components"),
            while_(token("+")).then([is(clause_select_component)
                .set("components")
                .hint("Trailing separators not allowed")]),
        ])
        .variables([IDENTIFIER_VAR, list_var("components"), node_var("foreign")])
        .build();

    let event_component = parser
        .grammar
        .new_node("event component")
        .rules([
            is(ident_path).commit().set(IDENTIFIER),
            maybe(alias).set("alias"),
        ])
        .variables([IDENTIFIER_VAR, node_var("alias")])
        .build();

    let clause_action = parser
        .grammar
        .new_node("action")
        .has(docstr, "docs")
        .rules([
            is(keyword("on")).commit(),
            is(ident).set(IDENTIFIER).commit(),
            is(token(":")),
            is(event_component).set("event"),
            while_(token("+")).then([is(event_component)
                .set("event")
                .hint("Trailing separators not allowed")]),
        ])
        .variables([IDENTIFIER_VAR, list_var("event")])
        .build();

    let clause_restriction = parser
        .grammar
        .new_node("restriction")
        .rules([
            is(keyword("where")).commit(),
            is(expression).set("expression"),
        ])
        .variables([node_var("expression")])
        .build();

    let clauses = parser
        .grammar
        .new_enum("clause")
        .options([clause_action, clause_restriction, clause_select])
        .build();

    let query = parser
        .grammar
        .new_node("query")
        .rules([
            is(token("(")).commit(),
            loop_().then([
                maybe(clauses).set("clauses"),
                is_one_of([
                    option(token(","))
                        .then([maybe(token(",")).fail(&grammar_errs::MULTIPLE_TRAILING_COMMAS)]),
                    option(token(")")).return_node(),
                ]),
            ]),
        ])
        .variables([list_var("clauses")])
        .build();

    let before_body = parser
        .grammar
        .new_node("before body")
        .rules([is(keyword("before")).commit(), is(code_body).set("body")])
        .variables([node_var("body")])
        .build();

    let after_body = parser
        .grammar
        .new_node("after body")
        .rules([is(keyword("after")).commit(), is(code_body).set("body")])
        .variables([node_var("body")])
        .build();

    let system = parser
        .grammar
        .new_node("system")
        .has(docstr, "docs")
        .has(access_modifier, "access mod")
        .rules([
            is(keyword("system")).commit().start(),
            is(ident).set(IDENTIFIER),
            maybe(generic_params).set("generic parameters"),
            is(query).set("query"),
            maybe(before_body).set("before body"),
            is(code_body)
                .set("main body")
                .hint("A system must contain main body"),
            maybe(after_body).set("after body"),
        ])
        .variables([
            IDENTIFIER_VAR,
            node_var("generic parameters"),
            node_var("query"),
            node_var("main body"),
            node_var("before body"),
            node_var("after body"),
        ])
        .build();

    let component = parser
        .grammar
        .new_node("component")
        .has(docstr, "docs")
        .has(access_modifier, "access mod")
        .rules([
            is(keyword("component")).commit().start(),
            is(ident).set(IDENTIFIER),
            maybe(token("=")).then([is(type_).set("type").important()]),
            is(end_stmt),
        ])
        .variables([IDENTIFIER_VAR, node_var("type")])
        .build();

    let resource = parser
        .grammar
        .new_node("resource")
        .has(docstr, "docs")
        .has(access_modifier, "access mod")
        .rules([
            is(keyword("resource")).commit().start(),
            is(ident).set(IDENTIFIER),
            maybe(token("?")).set("optional"),
            maybe(token(":")).then([is(type_)
                .set("type")
                .hint("Expected type literal after ':'")]),
            maybe(token("=")).then([is(expression)
                .set("default expression")
                .hint("Expected expression after '='")]),
        ])
        .variables([
            IDENTIFIER_VAR,
            node_var("optional"),
            node_var("type"),
            node_var("default expression"),
        ])
        .build();

    let type_kw = parser
        .grammar
        .new_node("type definition")
        .has(docstr, "docs")
        .has(access_modifier, "access mod")
        .rules([
            is(keyword("type")).commit().start(),
            is(ident).set(IDENTIFIER),
            maybe(generic_params).set("generic parameters"),
            maybe(token("=")).then([is(type_).set("type").important()]),
            is(end_stmt),
        ])
        .variables([
            IDENTIFIER_VAR,
            node_var("type"),
            node_var("generic parameters"),
        ])
        .build();

    let tls = parser
        .grammar
        .new_enum("top level statement")
        .options([
            function, system, component, type_kw, import, const_kw, trait_kw, impl_kw, resource,
            using,
        ])
        .build();

    parser
        .grammar
        .new_node("entry")
        .has(tl_docstr, "docs")
        .rules([loop_().then([is_one_of([
            option(tls).set("top level statements"),
            option(eof()).return_node(),
        ])])])
        .variables([list_var("top level statements")])
        .build();
    parser.parser.entry = Some("entry");
    parser.grammar.globals.push(list_var("imports"));

    let valid_result = Validator::default().validate(&parser);
    valid_result.print_all().unwrap();
    assert!(valid_result.pass());

    parser
}
