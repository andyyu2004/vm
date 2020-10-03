//! general use parsers

use super::*;
use crate::ast::*;
use crate::error::{ParseError, ParseResult};
use crate::lexer::{Base, LiteralKind, Tok, TokenType};
use crate::span::{with_source_map, Span};

/// parses an ident for a field access
pub struct FieldAccessParser {
    pub expr: P<Expr>,
}

impl<'a> Parse<'a> for FieldAccessParser {
    type Output = P<Expr>;

    fn parse(&mut self, parser: &mut Parser<'a>) -> ParseResult<'a, Self::Output> {
        let ident = if let Some(ident) = parser.accept_ident() {
            ident
        } else if let Some((kind, span)) = parser.accept_literal() {
            // tuple field access can have integer after the dot
            // `tuple.0`
            match kind {
                LiteralKind::Int { .. } => Ident::from(span),
                // have to deal with lexing/parsing ambiguities like tuple.1.1
                // which is lexed as [tuple . 1.1]
                LiteralKind::Float { .. } => {
                    let (x, y) = parser.split_float();
                    // replace the expression once, and then let the remainder of the code handle `y`
                    self.expr = parser.mk_expr(
                        self.expr.span.merge(x.span),
                        ExprKind::Field(std::mem::take(&mut self.expr), x),
                    );
                    y
                }
                _ => panic!("bad field access literal"),
            }
        } else {
            panic!("expected literal or identifier for field access")
        };
        Ok(parser.mk_expr(
            self.expr.span.merge(ident.span),
            ExprKind::Field(std::mem::take(&mut self.expr), ident),
        ))
    }
}

pub struct RetParser {
    pub ret_kw: Tok,
}

impl<'a> Parse<'a> for RetParser {
    type Output = P<Expr>;

    fn parse(&mut self, parser: &mut Parser<'a>) -> ParseResult<'a, Self::Output> {
        let expr = parser.try_parse(&mut ExprParser);
        let span = self
            .ret_kw
            .span
            .merge(expr.as_ref().map(|expr| expr.span).unwrap_or_else(|| parser.empty_span()));
        Ok(parser.mk_expr(span, ExprKind::Ret(expr)))
    }
}

pub struct FnSigParser {
    pub require_type_annotations: bool,
}

impl<'a> Parse<'a> for FnSigParser {
    type Output = FnSig;

    fn parse(&mut self, parser: &mut Parser<'a>) -> ParseResult<'a, Self::Output> {
        let require_type_annotations = self.require_type_annotations;
        parser.expect(TokenType::OpenParen)?;
        let mut param_parser = PunctuatedParser {
            inner: ParamParser { require_type_annotations },
            separator: TokenType::Comma,
        };
        let inputs = param_parser.parse(parser)?;
        parser.expect(TokenType::CloseParen)?;
        let mut output =
            parser.accept(TokenType::RArrow).map(|_arrow| parser.parse_ty(false)).transpose()?;

        if output.is_none() && !require_type_annotations {
            output = Some(parser.mk_infer_ty())
        }

        Ok(FnSig { params: inputs, ret_ty: output })
    }
}

/// parses function parameter <pat> (: <ty>)?
pub struct ParamParser {
    pub require_type_annotations: bool,
}

impl<'a> Parse<'a> for ParamParser {
    type Output = Param;

    fn parse(&mut self, parser: &mut Parser<'a>) -> ParseResult<'a, Self::Output> {
        let pattern = PatParser.parse(parser)?;
        let ty = if let Some(_colon) = parser.accept(TokenType::Colon) {
            parser.parse_ty(!self.require_type_annotations)
        } else if self.require_type_annotations {
            Err(parser.err(pattern.span, ParseError::RequireTypeAnnotations))
        } else {
            Ok(parser.mk_infer_ty())
        }?;
        Ok(Param { span: pattern.span.merge(ty.span), id: parser.mk_id(), pattern, ty })
    }
}

pub struct VisibilityParser;

impl<'a> Parse<'a> for VisibilityParser {
    type Output = Visibility;

    fn parse(&mut self, parser: &mut Parser<'a>) -> ParseResult<'a, Self::Output> {
        if let Some(pub_keyword) = parser.accept(TokenType::Pub) {
            Ok(Visibility { span: pub_keyword.span, node: VisibilityKind::Public })
        } else {
            Ok(Visibility { span: parser.empty_span(), node: VisibilityKind::Private })
        }
    }
}

/// implement Parser for TokenType to be used as a separator
impl<'a> Parse<'a> for TokenType {
    type Output = Tok;

    fn parse(&mut self, parser: &mut Parser<'a>) -> ParseResult<'a, Self::Output> {
        parser.expect(*self)
    }
}

/// parses a given parser zero or more times punctuated by some given separator parser
/// this parser accepts a trailing separator
///
/// <punctuated> = Ɛ | <inner> ( <sep> <inner> )* <sep>?
pub struct PunctuatedParser<P, S> {
    pub inner: P,
    pub separator: S,
}

impl<'a, P, S> Parse<'a> for PunctuatedParser<P, S>
where
    P: Parse<'a>,
    S: Parse<'a>,
{
    type Output = Vec<P::Output>;

    fn parse(&mut self, parser: &mut Parser<'a>) -> ParseResult<'a, Self::Output> {
        let mut vec = vec![];
        // if the first parse already fails then just return empty vector
        let p = match self.inner.parse(parser) {
            Ok(p) => p,
            Err(_) => return Ok(vec),
        };
        vec.push(p);
        while self.separator.parse(parser).is_ok() {
            match self.inner.parse(parser) {
                Ok(p) => vec.push(p),
                Err(_) => break,
            }
        }
        // parse the trailing separator if there is one
        let _ = self.separator.parse(parser);
        Ok(vec)
    }
}

/// similar to `PunctuatedParser` except a single element tuple must have a trailing comma (to
/// differentiate it from a parenthesization)
/// tbh, I'm not even sure what the real difference is now..
/// <tuple> = () | '(' ( <inner> , )+ <inner>? ')'
pub struct TupleParser<P> {
    pub inner: P,
}

impl<'a, P> Parse<'a> for TupleParser<P>
where
    P: Parse<'a>,
    P::Output: std::fmt::Debug,
{
    type Output = Vec<P::Output>;

    fn parse(&mut self, parser: &mut Parser<'a>) -> ParseResult<'a, Self::Output> {
        let mut vec = vec![];

        if parser.accept(TokenType::CloseParen).is_some() {
            return Ok(vec);
        }

        while parser.accept(TokenType::CloseParen).is_none() {
            vec.push(self.inner.parse(parser)?);
            if parser.accept(TokenType::Comma).is_none() {
                parser.expect(TokenType::CloseParen)?;
                break;
            }
        }
        Ok(vec)
    }
}

/// similar to `PunctuatedParser` except parses one or more occurences of `inner`
/// does not accept trailing separator
pub struct Punctuated1Parser<P, S> {
    pub inner: P,
    pub separator: S,
}

impl<'a, P, S> Parse<'a> for Punctuated1Parser<P, S>
where
    P: Parse<'a>,
    S: Parse<'a>,
{
    type Output = Vec<P::Output>;

    fn parse(&mut self, parser: &mut Parser<'a>) -> ParseResult<'a, Self::Output> {
        let mut vec = vec![self.inner.parse(parser)?];
        while self.separator.try_parse(parser).is_some() {
            vec.push(self.inner.parse(parser)?);
        }
        Ok(vec)
    }
}

/// parser some inner parser within parentheses
pub struct ParenParser<P> {
    pub inner: P,
}

impl<'a, P> Parse<'a> for ParenParser<P>
where
    P: Parse<'a>,
{
    type Output = P::Output;

    fn parse(&mut self, parser: &mut Parser<'a>) -> ParseResult<'a, Self::Output> {
        let p = self.inner.parse(parser)?;
        parser.expect(TokenType::CloseParen)?;
        Ok(p)
    }
}

pub struct StructExprParser {
    path: Path,
}

impl<'a> Parse<'a> for StructExprParser {
    type Output = P<Expr>;

    fn parse(&mut self, parser: &mut Parser<'a>) -> ParseResult<'a, Self::Output> {
        let field_parser = |parser: &mut Parser<'a>| {
            let ident = parser.expect_ident()?;
            let expr = if parser.accept(TokenType::Colon).is_some() {
                ExprParser.parse(parser)?
            } else {
                let span = parser.empty_span();
                // construct a Path node which the a single segment with ident `ident`
                // this is the implementation of the struct shorthand
                // S { t } -> S { t: t }
                let segment = PathSegment { id: parser.mk_id(), ident, args: None };
                let path = parser.mk_path(span, vec![segment]);
                parser.mk_expr(span, ExprKind::Path(path))
            };
            let span = ident.span.merge(expr.span);
            Ok(Field { ident, expr, span, id: parser.mk_id() })
        };
        let (span, fields) = PunctuatedParser { inner: field_parser, separator: TokenType::Comma }
            .spanned(true)
            .parse(parser)?;
        parser.expect(TokenType::CloseBrace)?;
        let path = std::mem::take(&mut self.path);
        Ok(parser.mk_expr(span, ExprKind::Struct(path, fields)))
    }
}

pub struct PathExprParser;

impl<'a> Parse<'a> for PathExprParser {
    type Output = P<Expr>;

    fn parse(&mut self, parser: &mut Parser<'a>) -> ParseResult<'a, Self::Output> {
        let path = parser.parse_path()?;
        let span = path.span;
        // if the path is immediately followed by an open brace, it may be a struct expr
        // SomeStruct {
        //    x: int,
        //    y: bool,
        // }
        // however, it could also be an identifier followed by a block
        if let Some(_) = parser.accept(TokenType::OpenBrace) {
            let mut struct_parser = StructExprParser { path };
            match struct_parser.try_parse(parser) {
                Some(struct_expr) => Ok(struct_expr),
                None => {
                    // need to backtrack past the open brace
                    // try_parse will handle the rest of the backtracking
                    parser.backtrack(1);
                    Ok(parser.mk_expr(span, ExprKind::Path(struct_parser.path)))
                }
            }
        } else {
            Ok(parser.mk_expr(span, ExprKind::Path(path)))
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum PathKind {
    /// arguments in expr position require disambiguation as follows
    /// this is due to ambiguity with comparision operators etc...
    /// a::b::<T>()
    Expr,
    /// types don't require the extra `::` as this ambiguity does not exists
    Type,
    /// it doesn't make sense to have generic parameters in module paths
    Module,
}

/// parses a path a::b::c
pub struct PathParser {
    pub kind: PathKind,
}

impl<'a> Parse<'a> for PathParser {
    type Output = Path;

    fn parse(&mut self, parser: &mut Parser<'a>) -> ParseResult<'a, Self::Output> {
        let (span, segments) = Punctuated1Parser {
            inner: PathSegmentParser { kind: self.kind },
            separator: TokenType::Dcolon,
        }
        .spanned(false)
        .parse(parser)?;
        // if the path is immediately followed by an open brace, it could be a struct expr
        Ok(parser.mk_path(span, segments))
    }
}

pub struct PathSegmentParser {
    kind: PathKind,
}

impl<'a> Parse<'a> for PathSegmentParser {
    type Output = PathSegment;

    fn parse(&mut self, parser: &mut Parser<'a>) -> ParseResult<'a, Self::Output> {
        let ident = parser.expect_ident()?;
        // TODO this does not deal with the where there is a preceding ::
        // and also does not handle the errors for expr position paths where :: is required
        let args = parser.parse_generic_args(self.kind)?;
        Ok(PathSegment { ident, id: parser.mk_id(), args })
    }
}

impl<'a> Parser<'a> {
    fn parse_generic_args(&mut self, kind: PathKind) -> ParseResult<'a, Option<GenericArgs>> {
        GenericArgsParser { kind }.parse(self)
    }
}

pub struct GenericArgsParser {
    kind: PathKind,
}

impl<'a> Parse<'a> for GenericArgsParser {
    type Output = Option<GenericArgs>;

    fn parse(&mut self, parser: &mut Parser<'a>) -> ParseResult<'a, Self::Output> {
        let lt = match parser.accept(TokenType::Lt) {
            Some(lt) => match self.kind {
                PathKind::Expr => {
                    parser.backtrack(1);
                    return Ok(None);
                }
                PathKind::Module => todo!(),
                PathKind::Type => lt,
            },
            None => return Ok(None),
        };
        let args =
            PunctuatedParser { inner: TyParser { allow_infer: true }, separator: TokenType::Comma }
                .parse(parser)?;
        let gt = parser.expect(TokenType::Gt)?;
        let span = lt.span.merge(gt.span);
        // if there are no arguments just treat it the same as if there was nothing there at all
        // i.e. a::b::<>::c <=> a::b::c
        Ok(if args.is_empty() { None } else { Some(GenericArgs { span, args }) })
    }
}

pub struct BlockParser {
    pub open_brace: Tok,
}

impl<'a> Parse<'a> for BlockParser {
    type Output = P<Block>;

    fn parse(&mut self, parser: &mut Parser<'a>) -> ParseResult<'a, Self::Output> {
        let mut stmts = vec![];
        let close_brace = loop {
            if let Some(close_brace) = parser.accept(TokenType::CloseBrace) {
                break close_brace;
            }
            match parser.parse_stmt() {
                Ok(stmt) => stmts.push(stmt),
                // recover as much as possible
                // find the next semicolon/brace
                Err(err) => {
                    err.emit();
                    loop {
                        match parser.peek().ttype {
                            TokenType::Eof =>
                                return Err(parser.err(parser.empty_span(), ParseError::Eof)),
                            TokenType::CloseBrace => break,
                            TokenType::Semi => {
                                parser.bump();
                                break;
                            }
                            _ => parser.bump(),
                        }
                    }
                }
            }
        };

        let span = self.open_brace.span.merge(close_brace.span);
        if !stmts.is_empty() {
            let len = stmts.len() - 1;
            // check there are no missing semicolons in expression statements
            for stmt in &stmts[..len] {
                if let StmtKind::Expr(_) = stmt.kind {
                    return Err(parser.err(stmt.span, ParseError::MissingSemi));
                }
            }
            // for easier typechecking when the final statement is diverging
            let expr = box stmts.pop().unwrap().upgrade_diverging_to_expr();
            stmts.push(expr);
        }

        Ok(box Block { span, id: parser.mk_id(), stmts })
    }
}

/// fn <ident>? (<pat>:<ty>) -> ret => <body>
/// "=>" is optional if body is a block expression
/// xs.map(fn(x) => y);
pub struct ClosureParser {
    pub fn_kw: Tok,
}

impl<'a> Parse<'a> for ClosureParser {
    type Output = P<Expr>;

    fn parse(&mut self, parser: &mut Parser<'a>) -> ParseResult<'a, Self::Output> {
        let name = parser.accept_ident();
        let sig = FnSigParser { require_type_annotations: false }.parse(parser)?;
        let body = if let Some(open_brace) = parser.accept(TokenType::OpenBrace) {
            let block = BlockParser { open_brace }.parse(parser)?;
            parser.mk_expr(block.span, ExprKind::Block(block))
        } else {
            parser.expect(TokenType::RFArrow)?;
            parser.parse_expr()?
        };
        let span = self.fn_kw.span.merge(body.span);
        Ok(parser.mk_expr(span, ExprKind::Closure(name, sig, body)))
    }
}

pub struct LiteralParser {
    pub kind: LiteralKind,
    pub span: Span,
}

impl<'a> Parse<'a> for LiteralParser {
    type Output = P<Expr>;

    fn parse(&mut self, parser: &mut Parser<'a>) -> ParseResult<'a, Self::Output> {
        let slice = with_source_map(|map| map.main_file()[self.span].to_owned());
        let literal = match self.kind {
            LiteralKind::Float { base, .. } => {
                if base != Base::Decimal {
                    panic!("only decimal float literals are supported")
                }
                Lit::Float(slice.parse().unwrap())
            }
            LiteralKind::Int { base, .. } =>
                Lit::Int(i64::from_str_radix(&slice, base as u32).unwrap()),
            _ => todo!(),
        };
        Ok(parser.mk_expr(self.span, ExprKind::Lit(literal)))
    }
}
pub struct ArmParser;

impl<'a> Parse<'a> for ArmParser {
    type Output = Arm;

    fn parse(&mut self, parser: &mut Parser<'a>) -> ParseResult<'a, Self::Output> {
        let pat = parser.parse_pattern()?;
        let guard = parser.accept(TokenType::If).map(|if_kw| parser.parse_expr()).transpose()?;
        let guard = None;
        parser.expect(TokenType::RFArrow)?;
        let body = parser.parse_expr()?;
        let span = pat.span.merge(body.span);
        Ok(Arm { id: parser.mk_id(), span, pat, body, guard })
    }
}

pub struct MatchParser {
    pub match_kw: Tok,
}

impl<'a> Parse<'a> for MatchParser {
    type Output = P<Expr>;

    fn parse(&mut self, parser: &mut Parser<'a>) -> ParseResult<'a, Self::Output> {
        let scrutinee = parser.parse_expr()?;
        parser.expect(TokenType::OpenBrace)?;
        let arms =
            PunctuatedParser { inner: ArmParser, separator: TokenType::Comma }.parse(parser)?;
        let brace = parser.expect(TokenType::CloseBrace)?;
        Ok(parser.mk_expr(self.match_kw.span.merge(brace.span), ExprKind::Match(scrutinee, arms)))
    }
}

pub struct IfParser {
    pub if_kw: Tok,
}

impl<'a> Parse<'a> for IfParser {
    type Output = P<Expr>;

    fn parse(&mut self, parser: &mut Parser<'a>) -> ParseResult<'a, Self::Output> {
        let cond = ExprParser.parse(parser)?;
        let open_brace = parser.expect(TokenType::OpenBrace)?;
        let thn = BlockParser { open_brace }.parse(parser)?;
        let els = if let Some(else_kw) = parser.accept(TokenType::Else) {
            Some(ElseParser { else_kw }.parse(parser)?)
        } else {
            None
        };
        let span = self.if_kw.span.merge(parser.empty_span());
        Ok(parser.mk_expr(span, ExprKind::If(cond, thn, els)))
    }
}

pub struct ElseParser {
    pub else_kw: Tok,
}

impl<'a> Parse<'a> for ElseParser {
    type Output = P<Expr>;

    fn parse(&mut self, parser: &mut Parser<'a>) -> ParseResult<'a, Self::Output> {
        if let Some(if_kw) = parser.accept(TokenType::If) {
            IfParser { if_kw }.parse(parser)
        } else if let Some(open_brace) = parser.accept(TokenType::OpenBrace) {
            let (span, block) = BlockParser { open_brace }.spanned(true).parse(parser)?;
            Ok(parser.mk_expr(span, ExprKind::Block(block)))
        } else {
            Err(parser.err(parser.empty_span(), ParseError::Unimpl))
        }
    }
}

pub struct GenericsParser;

impl<'a> Parse<'a> for GenericsParser {
    type Output = Generics;

    fn parse(&mut self, parser: &mut Parser<'a>) -> ParseResult<'a, Self::Output> {
        let mut span = parser.empty_span();
        let params = if parser.accept(TokenType::Lt).is_some() {
            let params = PunctuatedParser { inner: TyParamParser, separator: TokenType::Comma }
                .parse(parser)?;
            let gt = parser.expect(TokenType::Gt)?;
            span = span.merge(gt.span);
            params
        } else {
            vec![]
        };
        Ok(Generics { params, span })
    }
}

pub struct TyParamParser;

impl<'a> Parse<'a> for TyParamParser {
    type Output = TyParam;

    fn parse(&mut self, parser: &mut Parser<'a>) -> ParseResult<'a, Self::Output> {
        let ident = parser.expect_ident()?;
        let default = parser.accept(TokenType::Eq).map(|_| parser.parse_ty(false)).transpose()?;
        // eventually parse bounds here
        Ok(TyParam { span: ident.span, id: parser.mk_id(), ident, default })
    }
}