//! Checks every document Girens sends against the schema of the server
//! (`fixtures/schema.graphqls`, a copy of the server's
//! `api/src/main/resources/graphql/schema.graphqls`), so that a misspelt
//! field is found here and not by a user.

use std::collections::HashMap;

use graphql_parser::query::{self, Definition, OperationDefinition, Selection, SelectionSet};
use graphql_parser::schema::{self, Type, TypeDefinition};

/// The fields of a type, each with its arguments and the type it gives.
type Fields = HashMap<String, (Vec<String>, String)>;

fn base_type(of: &Type<'_, String>) -> String {
    match of {
        Type::NamedType(name) => name.clone(),
        Type::ListType(inner) | Type::NonNullType(inner) => base_type(inner),
    }
}

fn types() -> HashMap<String, Fields> {
    let text = include_str!("fixtures/schema.graphqls");
    let document = schema::parse_schema::<String>(text).expect("the schema parses");
    let mut types = HashMap::new();
    for definition in document.definitions {
        let schema::Definition::TypeDefinition(definition) = definition else {
            continue;
        };
        match definition {
            TypeDefinition::Object(object) => {
                let fields = object
                    .fields
                    .iter()
                    .map(|field| {
                        let arguments = field
                            .arguments
                            .iter()
                            .map(|argument| argument.name.clone())
                            .collect();
                        (field.name.clone(), (arguments, base_type(&field.field_type)))
                    })
                    .collect();
                types.insert(object.name, fields);
            }
            // A union has no fields of its own: only fragments apply.
            TypeDefinition::Union(union) => {
                types.insert(union.name, Fields::new());
            }
            _ => {}
        }
    }
    types
}

struct Checker<'a> {
    types: &'a HashMap<String, Fields>,
    fragments: HashMap<String, &'a query::FragmentDefinition<'a, String>>,
    problems: Vec<String>,
}

impl<'a> Checker<'a> {
    fn check(&mut self, set: &'a SelectionSet<'a, String>, of: &str) {
        for selection in &set.items {
            match selection {
                Selection::Field(field) if field.name == "__typename" => {}
                Selection::Field(field) => {
                    let Some((arguments, gives)) =
                        self.types.get(of).and_then(|fields| fields.get(&field.name))
                    else {
                        self.problems.push(format!("{of} has no field {}", field.name));
                        continue;
                    };
                    for (argument, _) in &field.arguments {
                        if !arguments.contains(argument) {
                            self.problems
                                .push(format!("{of}.{} takes no {argument}", field.name));
                        }
                    }
                    let gives = gives.clone();
                    match (self.types.contains_key(&gives), field.selection_set.items.is_empty()) {
                        (true, true) => self
                            .problems
                            .push(format!("{of}.{} needs a selection", field.name)),
                        (true, false) => self.check(&field.selection_set, &gives),
                        (false, false) => self
                            .problems
                            .push(format!("{of}.{} has nothing to select", field.name)),
                        (false, true) => {}
                    }
                }
                Selection::FragmentSpread(spread) => {
                    match self.fragments.get(&spread.fragment_name).copied() {
                        Some(fragment) => {
                            let query::TypeCondition::On(on) = &fragment.type_condition;
                            self.check(&fragment.selection_set, on);
                        }
                        None => self
                            .problems
                            .push(format!("no fragment {}", spread.fragment_name)),
                    }
                }
                Selection::InlineFragment(fragment) => {
                    let on = match &fragment.type_condition {
                        Some(query::TypeCondition::On(on)) => on.as_str(),
                        None => of,
                    };
                    if !self.types.contains_key(on) {
                        self.problems.push(format!("no type {on}"));
                        continue;
                    }
                    self.check(&fragment.selection_set, on);
                }
            }
        }
    }
}

#[test]
fn every_document_fits_the_schema() {
    let types = types();
    assert!(types.contains_key("Query") && types.contains_key("Mutation"));
    for text in girens_ister::documents() {
        let document = query::parse_query::<String>(text)
            .unwrap_or_else(|err| panic!("{text}\ndoes not parse: {err}"));
        let mut checker = Checker {
            types: &types,
            fragments: HashMap::new(),
            problems: Vec::new(),
        };
        for definition in &document.definitions {
            if let Definition::Fragment(fragment) = definition {
                checker.fragments.insert(fragment.name.clone(), fragment);
            }
        }
        let mut used = 0;
        for definition in &document.definitions {
            let Definition::Operation(operation) = definition else {
                continue;
            };
            match operation {
                OperationDefinition::Query(query) => checker.check(&query.selection_set, "Query"),
                OperationDefinition::Mutation(mutation) => {
                    checker.check(&mutation.selection_set, "Mutation")
                }
                _ => panic!("{text}\nis neither a query nor a mutation"),
            }
            used += 1;
        }
        assert_eq!(used, 1, "{text}\nshould hold one operation");
        assert!(
            checker.problems.is_empty(),
            "{text}\n{}",
            checker.problems.join("\n")
        );
        // A fragment the operation does not use is refused by the server.
        for name in checker.fragments.keys() {
            let spread = format!("...{name}");
            assert!(text.contains(&spread), "{text}\nnever uses fragment {name}");
        }
    }
}
