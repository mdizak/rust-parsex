use crate::query::Query;
use crate::token::Token;
use crate::token_iter::TokenIter;
use regex::RegexBuilder;
use std::collections::HashMap;

#[derive(Debug, Clone)]
pub struct Stack {
    tag_id: usize,
    parent_id: usize,
    depth: HashMap<String, Vec<usize>>,
    pub tokens: HashMap<usize, Token>,
    stack: HashMap<usize, Vec<usize>>,
    code: String,
    position: usize,
    parent_position: usize,
}

impl Stack {
    /// Instantiate a new token stack
    pub fn new(code: &str) -> Self {
        Self {
            tag_id: 0,
            parent_id: 0,
            depth: HashMap::new(),
            tokens: HashMap::new(),
            stack: HashMap::new(),
            code: code.to_owned(),
            position: 0,
            parent_position: 0,
        }
    }

    /// Push new token onto stack
    pub fn push(
        &mut self,
        tag: &str,
        attr_string: &str,
        is_single: &bool,
        tag_string: &str,
    ) -> usize {
        self.tag_id += 1;
        self.stack
            .entry(self.parent_id)
            .or_default()
            .push(self.tag_id);
        if !is_single {
            self.depth
                .entry(tag.to_string())
                .or_default()
                .push(self.tag_id);
        }

        // Add token
        let token_depth = if let Some(d) = self.depth.get(tag) {
            d.len()
        } else {
            0
        };
        let contents = if tag == "!" { tag_string } else { "" };
        self.tokens.insert(
            self.tag_id,
            Token::new(
                &self.tag_id,
                &self.parent_id,
                &token_depth,
                tag,
                attr_string,
                contents,
            ),
        );
        self.code = self
            .code
            .replacen(tag_string, format!("<parsex{}>", self.tag_id).as_str(), 1);

        self.parent_id = self.tag_id;
        self.tag_id
    }

    /// Close a previously opened HTML tag wwithin stack
    pub fn close_tag(&mut self, tag: &str, tag_string: &str) {
        if !self.depth.contains_key(tag) {
            return;
        }

        let tag_id: usize = self.depth.get_mut(tag).unwrap().pop().unwrap();
        if self.depth.get(tag).unwrap().is_empty() {
            self.depth.remove(tag);
        }
        self.code = self
            .code
            .replacen(tag_string, format!("</parsex{}>", &tag_id).as_str(), 1);

        // Update token as necessary
        let token = self.tokens.get_mut(&tag_id).unwrap();
        token.mark_closed();
        self.parent_id = token.parent_id();

        // Get body
        let search = format!("<parsex{}>(.*?)</parsex{}>", tag_id, tag_id);
        let re = RegexBuilder::new(&search)
            .dot_matches_new_line(true)
            .build()
            .unwrap();
        if let Some(m) = re.captures(&self.code) {
            token.set_contents(m.get(1).unwrap().as_str());
        }
    }

    /// Pull the next immutable token off the stack in hierarchial order, top to bottom, left to right
    pub fn pull(&mut self) -> Option<Token> {
        // Get next token
        if let Some(pos) = self.get_next_position() {
            let token = self.get(&pos).unwrap();
            return Some(token.clone());
        }

        None
    }

    /// Pull the next mutable token off the stack in hierarchial order, top to bottom, left to right
    pub fn pull_mut(&mut self) -> Option<&mut Token> {
        // Get next token
        if let Some(pos) = self.get_next_position() {
            return Some(self.get_mut(&pos).unwrap());
        }

        None
    }

    /// Retrieve single immutable token
    pub fn get(&mut self, token_id: &usize) -> Option<Token> {
        if let Some(token) = self.tokens.get(token_id) {
            return Some(token.clone());
        }
        None
    }

    /// Retrieve single mutable token
    pub fn get_mut(&mut self, token_id: &usize) -> Option<&mut Token> {
        self.tokens.get_mut(token_id)
    }

    /// Get next position to pull token from
    fn get_next_position(&mut self) -> Option<usize> {
        // Get next position
        let mut parent_id = self.position;
        loop {
            // Get children, and check if we hit bottom of tree.
            if parent_id == 0 && !self.stack.contains_key(&parent_id) {
                return None;
            } else if !self.stack.contains_key(&parent_id) {
                parent_id = self.tokens.get(&parent_id).unwrap().parent_id();
            }
            let mut children = self.stack.get(&parent_id).unwrap().clone();
            children.sort();

            // Get next index
            let index = match children.binary_search(&self.position) {
                Ok(r) => r + 1,
                Err(_) => 0,
            };

            // Check if done this set of tokens
            if index >= children.len()
                && ((parent_id == 0)
                    || (self.parent_position > 0 && parent_id == self.parent_position))
            {
                return None;
            } else if index >= children.len() {
                self.position = parent_id;
                parent_id = self.tokens.get(&parent_id).unwrap().parent_id();
                continue;
            }
            self.position = children[index];
            break;
        }

        Some(self.position)
    }

    /// Save token
    pub fn save(&mut self, token: &Token) {
        if let Some(item) = self.tokens.get_mut(&token.id()) {
            *item = token.clone();
        }
    }
    /// Iterate over all tokens in stack in hierarchial order, top to bottom, left to right
    pub fn iter(&mut self) -> TokenIter {
        Query::new(self).iter()
    }
    // Query tags by desired criteria
    pub fn query(&mut self) -> Query {
        Query::new(self)
    }

    // Get children tokens, must call .iter() or .to_vec() on this result
    pub fn get_children(&mut self, token_id: &usize) -> Query {
        self.query().parent_id(token_id)
    }

    /// Set parent id for next pull of all tokens
    pub fn set_parent_position(&mut self, parent_id: &usize) {
        self.position = *parent_id;
        self.parent_position = *parent_id;
    }

    /// Render stack, and return resulting HTML including any modifications made to stack.
    pub fn render(&mut self) -> String {
        // Initialize
        let mut html = self.code.clone();
        self.set_parent_position(&0);

        // Go through  stack
        while let Some(token) = self.pull() {
            // Get attribute string
            let attr_string: String = token
                .attributes()
                .iter()
                .map(|(key, value)| format!("{}=\"{}\"", key, value))
                .collect::<Vec<String>>()
                .join(" ");

            // Get replace string
            let open_tag = format!("<{} {} {}", token.tag(), attr_string, token.attr_extra());
            let replace_str = if token.is_closed() {
                format!(
                    "{}>{}</{}>",
                    open_tag.trim_end(),
                    token.contents(),
                    token.tag()
                )
            } else {
                format!("{}/>", open_tag)
            };

            // Quick replace if comment or tag not closed
            if token.tag() == "!" {
                let search = format!("<parsex{}>", token.id());
                html = html.replace(&search, token.contents().as_str());
                continue;
            } else if !token.is_closed() {
                let search = format!("<parsex{}>", token.id());
                html = html.replace(&search, &replace_str);
                continue;
            }

            // Replace open / close tag
            let search = format!(r"(?s)<parsex{}>.*?</parsex{}>", token.id(), token.id());
            let re = RegexBuilder::new(&search)
                .dot_matches_new_line(true)
                .build()
                .unwrap();
            html = re.replace_all(&html, &replace_str).to_string();
        }

        html
    }
}
