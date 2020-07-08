use super::{parsers::ParenParser, Parse, Parser};
use crate::ast::{Pattern, PatternKind, P};
use crate::error::ParseResult;
use crate::lexer::TokenType;

pub struct PatternParser;

impl Parse for PatternParser {
    type Output = P<Pattern>;
    fn parse(&mut self, parser: &mut Parser) -> ParseResult<Self::Output> {
        if let Some(token) = parser.accept(TokenType::Underscore) {
            Ok(parser.mk_pat(token.span, PatternKind::Wildcard))
        } else if let Some(ident) = parser.accept_ident() {
            Ok(parser.mk_pat(ident.span, PatternKind::Ident(ident, None)))
        } else if let Some(open_paren) = parser.accept(TokenType::OpenParen) {
            let (pattern, span) = ParenParser { open_paren, inner: PatternParser }.parse(parser)?;
            Ok(parser.mk_pat(span, PatternKind::Paren(pattern)))
        } else {
            todo!()
        }
    }
}
