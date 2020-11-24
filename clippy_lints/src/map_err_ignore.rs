use crate::utils::span_lint_and_help;

use rustc_hir::{CaptureBy, Expr, ExprKind, PatKind, QPath, def::Res, def::DefKind, def::CtorKind, def::CtorOf};
use rustc_lint::{LateContext, LateLintPass};
use rustc_session::{declare_lint_pass, declare_tool_lint};

declare_clippy_lint! {
    /// **What it does:** Checks for instances of `map_err(|_| Some::Enum)`
    ///
    /// **Why is this bad?** This map_err throws away the original error rather than allowing the enum to contain and report the cause of the error
    ///
    /// **Known problems:** None.
    ///
    /// **Example:**
    /// Before:
    /// ```rust
    /// use std::fmt;
    ///
    /// #[derive(Debug)]
    /// enum Error {
    ///     Indivisible,
    ///     Remainder(u8),
    /// }
    ///
    /// impl fmt::Display for Error {
    ///     fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
    ///         match self {
    ///             Error::Indivisible => write!(f, "could not divide input by three"),
    ///             Error::Remainder(remainder) => write!(
    ///                 f,
    ///                 "input is not divisible by three, remainder = {}",
    ///                 remainder
    ///             ),
    ///         }
    ///     }
    /// }
    ///
    /// impl std::error::Error for Error {}
    ///
    /// fn divisible_by_3(input: &str) -> Result<(), Error> {
    ///     input
    ///         .parse::<i32>()
    ///         .map_err(|_| Error::Indivisible)
    ///         .map(|v| v % 3)
    ///         .and_then(|remainder| {
    ///             if remainder == 0 {
    ///                 Ok(())
    ///             } else {
    ///                 Err(Error::Remainder(remainder as u8))
    ///             }
    ///         })
    /// }
    ///  ```
    ///
    ///  After:
    ///  ```rust
    /// use std::{fmt, num::ParseIntError};
    ///
    /// #[derive(Debug)]
    /// enum Error {
    ///     Indivisible(ParseIntError),
    ///     Remainder(u8),
    /// }
    ///
    /// impl fmt::Display for Error {
    ///     fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
    ///         match self {
    ///             Error::Indivisible(_) => write!(f, "could not divide input by three"),
    ///             Error::Remainder(remainder) => write!(
    ///                 f,
    ///                 "input is not divisible by three, remainder = {}",
    ///                 remainder
    ///             ),
    ///         }
    ///     }
    /// }
    ///
    /// impl std::error::Error for Error {
    ///     fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
    ///         match self {
    ///             Error::Indivisible(source) => Some(source),
    ///             _ => None,
    ///         }
    ///     }
    /// }
    ///
    /// fn divisible_by_3(input: &str) -> Result<(), Error> {
    ///     input
    ///         .parse::<i32>()
    ///         .map_err(Error::Indivisible)
    ///         .map(|v| v % 3)
    ///         .and_then(|remainder| {
    ///             if remainder == 0 {
    ///                 Ok(())
    ///             } else {
    ///                 Err(Error::Remainder(remainder as u8))
    ///             }
    ///         })
    /// }
    /// ```
    pub MAP_ERR_IGNORE,
    pedantic,
    "`map_err` should not ignore the original error"
}

declare_lint_pass!(MapErrIgnore => [MAP_ERR_IGNORE]);

fn is_unit_enum_variant(input: &ExprKind<'_>) -> bool {
    match input {
        ExprKind::Path(qpath) => {
            match qpath {
                QPath::Resolved(None, enum_path) => {
                    match enum_path.res {
                        // the definition should be a enum constructor with a 
                        // Const (unit) enum variant (and we do not want to match on the `DefId`)
                        Res::Def(DefKind::Ctor(CtorOf::Variant, CtorKind::Const), _) => true,
                        _ => false,
                    }
                }, 
                // If this is not a resolved qualified path it isn't a unit enum
                _ => false,    
            }
        }
        // if this expression isn't a path it isn't an enum 
        _ => false,
    }
}

impl<'tcx> LateLintPass<'tcx> for MapErrIgnore {
    // do not try to lint if this is from a macro or desugaring
    fn check_expr(&mut self, cx: &LateContext<'_>, e: &Expr<'_>) {
        if e.span.from_expansion() {
            return;
        }

        // check if this is a method call (e.g. x.foo())
        if let ExprKind::MethodCall(ref method, _t_span, ref args, _) = e.kind {
            // only work if the method name is `map_err` and there are only 2 arguments (e.g. x.map_err(|_|[1]
            // Enum::Variant[2]))
            if method.ident.as_str() == "map_err" && args.len() == 2 {
                // make sure the first argument is a closure, and grab the CaptureRef, body_id, and body_span fields
                if let ExprKind::Closure(capture, _, body_id, body_span, _) = args[1].kind {
                    // check if this is by Reference (meaning there's no move statement)
                    if capture == CaptureBy::Ref {
                        // Get the closure body to check the parameters and values
                        let closure_body = cx.tcx.hir().body(body_id);
                        // make sure there's only one parameter (`|_|`)
                        if closure_body.params.len() == 1 {
                            // make sure that parameter is the wild token (`_`)
                            if let PatKind::Wild = closure_body.params[0].pat.kind {
                                // check the value of the body is only a unit enum 
                                if is_unit_enum_variant(&closure_body.value.kind) {
                                    // span the area of the closure capture and warn that the
                                    // original error will be thrown away
                                    span_lint_and_help(
                                        cx,
                                        MAP_ERR_IGNORE,
                                        body_span,
                                        "`map_err(|_|...` ignores the original error",
                                        None,
                                        "Consider wrapping the error in an enum variant",
                                    );
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}
