use std::fmt;

/// The `pb` parameter of Google Maps: a Protocol Buffers message flattened to text. Each field is
/// `!<number><type><value>`; a message (`m`) is followed by its fields, and its value is the
/// number of tokens it holds, counting those of the messages within it.
///
/// Held as a tree, so that adding or removing fields keeps every count right: the counts are
/// computed when the text is written.
#[derive(Debug, Clone, PartialEq)]
pub struct Pb {
    pub nodes: Vec<PbNode>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum PbNode {
    /// A scalar: its number, its type letter (`d` double, `f` float, `i` int, `u` uint, `e`
    /// enum, `b` bool, `s` string, …) and its text.
    Field {
        number: u32,
        kind: char,
        value: String,
    },
    Message {
        number: u32,
        children: Vec<PbNode>,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PbError {
    /// A token that is not `!<number><letter><value>`.
    InvalidToken(String),
    /// A message whose count runs past the end of the text, or is not a number.
    InvalidCount(String),
}

impl fmt::Display for PbError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            PbError::InvalidToken(t) => write!(f, "invalid pb token {t}"),
            PbError::InvalidCount(t) => write!(f, "invalid pb message count {t}"),
        }
    }
}

impl std::error::Error for PbError {}

impl PbNode {
    pub fn field(number: u32, kind: char, value: impl ToString) -> PbNode {
        PbNode::Field { number, kind, value: value.to_string() }
    }

    pub fn message(number: u32, children: Vec<PbNode>) -> PbNode {
        PbNode::Message { number, children }
    }

    pub fn number(&self) -> u32 {
        match self {
            PbNode::Field { number, .. } | PbNode::Message { number, .. } => *number,
        }
    }

    /// The number of tokens of this node, itself included.
    fn tokens(&self) -> usize {
        match self {
            PbNode::Field { .. } => 1,
            PbNode::Message { children, .. } => 1 + children.iter().map(PbNode::tokens).sum::<usize>(),
        }
    }

    fn write(&self, out: &mut String) {
        match self {
            PbNode::Field { number, kind, value } => {
                out.push('!');
                out.push_str(&number.to_string());
                out.push(*kind);
                out.push_str(value);
            }
            PbNode::Message { number, children } => {
                out.push('!');
                out.push_str(&number.to_string());
                out.push('m');
                out.push_str(&(self.tokens() - 1).to_string());
                for child in children {
                    child.write(out);
                }
            }
        }
    }
}

impl Pb {
    /// Reads the text of a `pb`.
    pub fn parse(text: &str) -> Result<Pb, PbError> {
        let tokens = Self::tokens(text)?;
        let mut position = 0;
        let nodes = Self::read(&tokens, &mut position, tokens.len())?;
        Ok(Pb { nodes })
    }

    fn tokens(text: &str) -> Result<Vec<(u32, char, &str)>, PbError> {
        let Some(rest) = text.strip_prefix('!') else {
            return if text.is_empty() { Ok(Vec::new()) } else { Err(PbError::InvalidToken(text.chars().take(20).collect())) };
        };
        rest.split('!')
            .map(|token| {
                let digits = token.bytes().take_while(u8::is_ascii_digit).count();
                let number = token[..digits].parse().map_err(|_| PbError::InvalidToken(token.into()))?;
                let mut chars = token[digits..].chars();
                let kind = chars.next().filter(char::is_ascii_alphabetic).ok_or_else(|| PbError::InvalidToken(token.into()))?;
                Ok((number, kind, chars.as_str()))
            })
            .collect()
    }

    fn read(tokens: &[(u32, char, &str)], position: &mut usize, end: usize) -> Result<Vec<PbNode>, PbError> {
        let mut nodes = Vec::new();
        while *position < end {
            let (number, kind, value) = tokens[*position];
            *position += 1;
            if kind == 'm' {
                let count: usize = value.parse().map_err(|_| PbError::InvalidCount(value.into()))?;
                let inner_end = position.checked_add(count).filter(|e| *e <= end).ok_or_else(|| PbError::InvalidCount(value.into()))?;
                let children = Self::read(tokens, position, inner_end)?;
                nodes.push(PbNode::Message { number, children });
            } else {
                nodes.push(PbNode::Field { number, kind, value: value.into() });
            }
        }
        Ok(nodes)
    }

    /// Escapes a string value: `*` as `*2A`, then `!` as `*21`.
    pub fn escape(text: &str) -> String {
        text.replace('*', "*2A").replace('!', "*21")
    }

    /// The node at `path`, a list of indices into the children of each level.
    pub fn node_mut(&mut self, path: &[usize]) -> Option<&mut PbNode> {
        let (first, rest) = path.split_first()?;
        let mut node = self.nodes.get_mut(*first)?;
        for index in rest {
            node = match node {
                PbNode::Message { children, .. } => children.get_mut(*index)?,
                PbNode::Field { .. } => return None,
            };
        }
        Some(node)
    }

    /// The index among `nodes` of the first message numbered `number` whose first child is
    /// numbered `first_child` (to tell apart repeated messages of one number).
    pub fn find_message(nodes: &[PbNode], number: u32, first_child: Option<u32>) -> Option<usize> {
        nodes.iter().position(|n| match n {
            PbNode::Message { number: m, children } => *m == number && first_child.is_none_or(|c| children.first().map(PbNode::number) == Some(c)),
            PbNode::Field { .. } => false,
        })
    }

    /// Inserts `node` among `children` in the order of field numbers: before the first child of a
    /// larger number. A scalar of the same number and type is replaced instead.
    pub fn insert_ordered(children: &mut Vec<PbNode>, node: PbNode) {
        if let PbNode::Field { number, kind, .. } = &node
            && let Some(existing) = children.iter_mut().find(|c| matches!(c, PbNode::Field { number: n, kind: k, .. } if n == number && k == kind))
        {
            *existing = node;
            return;
        }
        let at = children.iter().position(|c| c.number() > node.number()).unwrap_or(children.len());
        children.insert(at, node);
    }
}

impl fmt::Display for Pb {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mut out = String::new();
        for node in &self.nodes {
            node.write(&mut out);
        }
        f.write_str(&out)
    }
}
