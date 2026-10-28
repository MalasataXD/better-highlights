//! Cognitive complexity, after SonarSource's spec (https://www.sonarsource.com/docs/CognitiveComplexity.pdf).
//! Labelled jumps and recursion are not counted.

use tree_sitter::{Node, Tree};

use crate::languages::Language;

/// Complexity of one function, as the list of constructs that add to it.
#[derive(Debug, PartialEq)]
pub struct FunctionComplexity {
    /// The function's name, or for lambdas its parameters; where hints and diagnostics go.
    pub anchor: tree_sitter::Range,
    pub increments: Vec<Increment>,
}

impl FunctionComplexity {
    pub fn score(&self) -> u32 {
        self.increments.iter().map(|increment| increment.cost).sum()
    }

    /// Increments grouped under the outermost construct containing them, costliest first.
    pub fn hotspots(&self) -> Vec<Hotspot> {
        let mut hotspots: Vec<Hotspot> = Vec::new();
        // Increments are in pre-order, so anything inside the last hotspot comes right after it.
        for increment in &self.increments {
            match hotspots.last_mut() {
                Some(hotspot) if increment.range.end_byte <= hotspot.range.end_byte => {
                    hotspot.cost += increment.cost;
                    hotspot.nesting += increment.nesting;
                }
                _ => hotspots.push(Hotspot {
                    range: increment.range,
                    cost: increment.cost,
                    nesting: increment.nesting,
                }),
            }
        }
        hotspots.sort_by_key(|hotspot| std::cmp::Reverse(hotspot.cost));
        hotspots
    }
}

/// One construct's contribution, e.g. a `for` nested two deep costs 3 with `nesting` 2.
#[derive(Debug, PartialEq)]
pub struct Increment {
    pub range: tree_sitter::Range,
    pub cost: u32,
    pub nesting: u32,
}

/// A top-level construct plus everything nested in it: the unit you would extract or flatten.
#[derive(Debug, PartialEq)]
pub struct Hotspot {
    pub range: tree_sitter::Range,
    pub cost: u32,
    /// The part of `cost` that comes from nesting.
    pub nesting: u32,
}

/// Scores every function in `tree`: named functions anywhere, and lambdas that are not inside
/// another function.
pub fn functions(tree: &Tree, source: &str, language: &Language) -> Vec<FunctionComplexity> {
    let mut found = Vec::new();
    collect(tree.root_node(), false, source, language, &mut found);
    found
}

fn collect(
    node: Node,
    in_function: bool,
    source: &str,
    language: &Language,
    found: &mut Vec<FunctionComplexity>,
) {
    let kind = node.kind();
    let is_function = language.functions.contains(&kind);
    let is_lambda = language.lambdas.contains(&kind);
    if is_function || (is_lambda && !in_function) {
        let mut scorer = Scorer {
            source,
            language,
            increments: Vec::new(),
        };
        scorer.visit_children(node, 0);
        let anchor = ["name", "parameters", "parameter"]
            .iter()
            .find_map(|field| node.child_by_field_name(field))
            .unwrap_or(node)
            .range();
        found.push(FunctionComplexity {
            anchor,
            increments: scorer.increments,
        });
    }
    let mut cursor = node.walk();
    for child in node.named_children(&mut cursor) {
        collect(
            child,
            in_function || is_function || is_lambda,
            source,
            language,
            found,
        );
    }
}

struct Scorer<'a> {
    source: &'a str,
    language: &'a Language,
    increments: Vec<Increment>,
}

impl Scorer<'_> {
    fn add(&mut self, node: Node, cost: u32, nesting: u32) {
        self.increments.push(Increment {
            range: node.range(),
            cost: cost + nesting,
            nesting,
        });
    }

    fn visit_children(&mut self, node: Node, nesting: u32) {
        let mut cursor = node.walk();
        for child in node.named_children(&mut cursor) {
            self.visit(child, nesting);
        }
    }

    fn visit(&mut self, node: Node, nesting: u32) {
        let kind = node.kind();
        let language = self.language;
        if language.functions.contains(&kind) {
            // Scored on its own.
        } else if language.lambdas.contains(&kind) {
            self.visit_children(node, nesting + 1);
        } else if language.ifs.contains(&kind) {
            self.visit_if(node, nesting, false);
        } else if language.nesting.contains(&kind) {
            self.add(node, 1, nesting);
            self.visit_children(node, nesting + 1);
        } else if self.logical_operator(node).is_some() {
            self.visit_logical(node, nesting);
        } else {
            self.visit_children(node, nesting);
        }
    }

    /// `else if` costs a flat +1 and continues the chain at the same nesting.
    fn visit_if(&mut self, node: Node, nesting: u32, is_else_if: bool) {
        self.add(node, 1, if is_else_if { 0 } else { nesting });
        let condition = node.child_by_field_name("condition");
        // Several in Python: one per `elif`, plus the `else`.
        let mut cursor = node.walk();
        let alternatives: Vec<_> = node
            .children_by_field_name("alternative", &mut cursor)
            .collect();
        for child in node.named_children(&mut cursor) {
            if alternatives.contains(&child) {
                self.visit_else(child, nesting);
            } else if Some(child) == condition {
                self.visit(child, nesting);
            } else {
                self.visit(child, nesting + 1);
            }
        }
    }

    fn visit_else(&mut self, alternative: Node, nesting: u32) {
        let body = if self.language.else_clause == Some(alternative.kind()) {
            let mut cursor = alternative.walk();
            let Some(body) = alternative
                .named_children(&mut cursor)
                .find(|child| !self.language.comments.contains(&child.kind()))
            else {
                return;
            };
            body
        } else {
            alternative
        };
        if self.language.ifs.contains(&body.kind()) {
            self.visit_if(body, nesting, true);
        } else {
            self.add(alternative, 1, 0);
            self.visit(body, nesting + 1);
        }
    }

    /// A run of `&&`/`||` costs +1, plus +1 each time the operator changes.
    /// Parenthesised groups start a run of their own.
    fn visit_logical(&mut self, node: Node, nesting: u32) {
        let mut operators = Vec::new();
        let mut operands = Vec::new();
        self.flatten_logical(node, &mut operators, &mut operands);
        let changes = operators
            .windows(2)
            .filter(|pair| pair[0] != pair[1])
            .count() as u32;
        self.add(node, 1 + changes, 0);
        for operand in operands {
            self.visit(operand, nesting);
        }
    }

    fn flatten_logical<'t>(
        &self,
        node: Node<'t>,
        operators: &mut Vec<&'static str>,
        operands: &mut Vec<Node<'t>>,
    ) {
        let Some(operator) = self.logical_operator(node) else {
            operands.push(node);
            return;
        };
        let (Some(left), Some(right)) = (
            node.child_by_field_name("left"),
            node.child_by_field_name("right"),
        ) else {
            return;
        };
        self.flatten_logical(left, operators, operands);
        operators.push(operator);
        self.flatten_logical(right, operators, operands);
    }

    /// `&&` or `||` when `node` is a logical operation; Python's `and`/`or` map onto them.
    fn logical_operator(&self, node: Node) -> Option<&'static str> {
        if node.kind() != self.language.logical {
            return None;
        }
        let operator = node.child_by_field_name("operator")?;
        match &self.source[operator.byte_range()] {
            "&&" | "and" => Some("&&"),
            "||" | "or" => Some("||"),
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use super::*;
    use crate::languages;

    fn functions_in(file: &str, source: &str) -> Vec<FunctionComplexity> {
        let language = languages::for_path(Path::new(file)).unwrap();
        functions(&language.parse(source).unwrap(), source, &language)
    }

    /// `(anchor row, score)` per function.
    fn scores(file: &str, source: &str) -> Vec<(usize, u32)> {
        functions_in(file, source)
            .iter()
            .map(|function| (function.anchor.start_point.row, function.score()))
            .collect()
    }

    #[test]
    fn counts_branches_nesting_and_logical_runs() {
        let source = "
function f(a, b, c) {
  if (a && b || c) {
    for (const x of b) {
      b.map(y => y ? 1 : 2);
    }
  } else if (b) {
  } else {
  }
}";
        let [function] = functions_in("a.ts", source).try_into().unwrap();
        let rows_and_costs = |items: Vec<(tree_sitter::Range, u32)>| {
            items
                .into_iter()
                .map(|(range, cost)| (range.start_point.row, cost))
                .collect::<Vec<_>>()
        };
        // if, `&& ||`, for, ternary inside the lambda, else if, else.
        assert_eq!(
            rows_and_costs(
                function
                    .increments
                    .iter()
                    .map(|i| (i.range, i.cost))
                    .collect()
            ),
            vec![(2, 1), (2, 2), (3, 2), (4, 4), (6, 1), (7, 1)]
        );
        // All of it sits inside the top-level `if`.
        assert_eq!(
            rows_and_costs(
                function
                    .hotspots()
                    .iter()
                    .map(|h| (h.range, h.cost))
                    .collect()
            ),
            vec![(2, 11)]
        );
    }

    #[test]
    fn handles_else_without_a_wrapper_node() {
        let source = "
class C {
    int M(int a) {
        if (a > 0) { return 1; }
        else if (a < 0) { return -1; }
        else { while (a == 0) { a++; } }
        return a > 1 ? 1 : 0;
    }
}";
        assert_eq!(scores("a.cs", source), vec![(2, 6)]);
    }

    #[test]
    fn scores_nested_named_functions_separately() {
        let source = "
fn outer(x: Option<i32>) -> i32 {
    fn inner(y: i32) -> bool { y > 0 && y < 10 }
    match x {
        Some(v) if inner(v) => v,
        _ => 0,
    }
}";
        assert_eq!(scores("a.rs", source), vec![(1, 1), (2, 1)]);
    }

    #[test]
    fn counts_each_python_elif_and_boolean_run() {
        let source = "
def f(a, b):
    if a and b or a:
        pass
    elif b:
        pass
    elif a:
        pass
    else:
        pass
";
        // if 1, `and or` 2, elif 1, elif 1, else 1.
        assert_eq!(scores("a.py", source), vec![(1, 6)]);
    }
}
