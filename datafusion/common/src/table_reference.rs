// Licensed to the Apache Software Foundation (ASF) under one
// or more contributor license agreements.  See the NOTICE file
// distributed with this work for additional information
// regarding copyright ownership.  The ASF licenses this file
// to you under the Apache License, Version 2.0 (the
// "License"); you may not use this file except in compliance
// with the License.  You may obtain a copy of the License at
//
//   http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing,
// software distributed under the License is distributed on an
// "AS IS" BASIS, WITHOUT WARRANTIES OR CONDITIONS OF ANY
// KIND, either express or implied.  See the License for the
// specific language governing permissions and limitations
// under the License.

use crate::utils::{parse_identifiers_normalized, quote_identifier};
use std::sync::Arc;

/// A fully resolved path to a table of the form "catalog.schema.table"
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct ResolvedTableReference {
    /// The catalog (aka database) containing the table
    pub catalog: Arc<str>,
    /// The schema containing the table
    pub schema: Arc<str>,
    /// The table name
    pub table: Arc<str>,
}

impl std::fmt::Display for ResolvedTableReference {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}.{}.{}", self.catalog, self.schema, self.table)
    }
}

/// A multi part identifier (path) to a table that may require further
/// resolution (e.g. `foo.bar`).
///
/// [`TableReference`]s are cheap to `clone()` as they are implemented with
/// `Arc`.
///
/// See [`ResolvedTableReference`] for a fully resolved table reference.
///
/// # Creating [`TableReference`]
///
/// When converting strings to [`TableReference`]s, the string is parsed as
/// though it were a SQL identifier, normalizing (convert to lowercase) any
/// unquoted identifiers.  [`TableReference::bare`] creates references without
/// applying normalization semantics.
///
/// # Examples
/// ```
/// # use datafusion_common::TableReference;
/// // Get a table reference to 'mytable'
/// let table_reference = TableReference::from("mytable");
/// assert_eq!(table_reference, TableReference::bare("mytable"));
///
/// // Get a table reference to 'mytable' (note the capitalization)
/// let table_reference = TableReference::from("MyTable");
/// assert_eq!(table_reference, TableReference::bare("mytable"));
///
/// // Get a table reference to 'MyTable' (note the capitalization) using double quotes
/// // (programmatically it is better to use `TableReference::bare` for this)
/// let table_reference = TableReference::from(r#""MyTable""#);
/// assert_eq!(table_reference, TableReference::bare("MyTable"));
///
/// // Get a table reference to 'myschema.mytable' (note the capitalization)
/// let table_reference = TableReference::from("MySchema.MyTable");
/// assert_eq!(table_reference, TableReference::partial("myschema", "mytable"));
///```
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum TableReference {
    /// An unqualified table reference, e.g. "table"
    Bare {
        /// The table name
        table: Arc<str>,
    },
    /// A partially resolved table reference, e.g. "schema.table"
    Partial {
        /// The schema containing the table
        schema: Arc<str>,
        /// The table name
        table: Arc<str>,
    },
    /// A fully resolved table reference, e.g. "catalog.schema.table"
    Full {
        /// The catalog (aka database) containing the table
        catalog: Arc<str>,
        /// The schema containing the table
        schema: Arc<str>,
        /// The table name
        table: Arc<str>,
    },
}

impl std::fmt::Display for TableReference {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            TableReference::Bare { table } => write!(f, "{table}"),
            TableReference::Partial { schema, table } => {
                write!(f, "{schema}.{table}")
            }
            TableReference::Full {
                catalog,
                schema,
                table,
            } => write!(f, "{catalog}.{schema}.{table}"),
        }
    }
}

impl TableReference {
    /// Convenience method for creating a typed none `None`
    pub fn none() -> Option<TableReference> {
        None
    }

    /// Convenience method for creating a [`TableReference::Bare`]
    ///
    /// As described on [`TableReference`] this does *NO* normalization at
    /// all, so "Foo.Bar" stays as a reference to the table named
    /// "Foo.Bar" (rather than "foo"."bar")
    pub fn bare(table: impl Into<Arc<str>>) -> TableReference {
        TableReference::Bare {
            table: table.into(),
        }
    }

    /// Convenience method for creating a [`TableReference::Partial`].
    ///
    /// Note: *NO* normalization is applied to the schema or table name.
    pub fn partial(
        schema: impl Into<Arc<str>>,
        table: impl Into<Arc<str>>,
    ) -> TableReference {
        TableReference::Partial {
            schema: schema.into(),
            table: table.into(),
        }
    }

    /// Convenience method for creating a [`TableReference::Full`]
    ///
    /// Note: *NO* normalization is applied to the catalog, schema or table
    /// name.
    pub fn full(
        catalog: impl Into<Arc<str>>,
        schema: impl Into<Arc<str>>,
        table: impl Into<Arc<str>>,
    ) -> TableReference {
        TableReference::Full {
            catalog: catalog.into(),
            schema: schema.into(),
            table: table.into(),
        }
    }

    /// Retrieve the table name, regardless of qualification.
    pub fn table(&self) -> &str {
        match self {
            Self::Full { table, .. }
            | Self::Partial { table, .. }
            | Self::Bare { table } => table,
        }
    }

    /// Retrieve the schema name if [`Self::Partial]` or [`Self::`Full`],
    /// `None` otherwise.
    pub fn schema(&self) -> Option<&str> {
        match self {
            Self::Full { schema, .. } | Self::Partial { schema, .. } => Some(schema),
            _ => None,
        }
    }

    /// Retrieve the catalog name if  [`Self::Full`], `None` otherwise.
    pub fn catalog(&self) -> Option<&str> {
        match self {
            Self::Full { catalog, .. } => Some(catalog),
            _ => None,
        }
    }

    /// Compare with another [`TableReference`] as if both are resolved.
    /// This allows comparing across variants. If a field is not present
    /// in both variants being compared then it is ignored in the comparison.
    ///
    /// e.g. this allows a [`TableReference::Bare`] to be considered equal to a
    /// fully qualified [`TableReference::Full`] if the table names match.
    pub fn resolved_eq(&self, other: &Self) -> bool {
        match self {
            TableReference::Bare { table } => **table == *other.table(),
            TableReference::Partial { schema, table } => {
                **table == *other.table() && other.schema().is_none_or(|s| *s == **schema)
            }
            TableReference::Full {
                catalog,
                schema,
                table,
            } => {
                **table == *other.table()
                    && other.schema().is_none_or(|s| *s == **schema)
                    && other.catalog().is_none_or(|c| *c == **catalog)
            }
        }
    }

    /// Given a default catalog and schema, ensure this table reference is fully
    /// resolved
    pub fn resolve(
        self,
        default_catalog: &str,
        default_schema: &str,
    ) -> ResolvedTableReference {
        match self {
            Self::Full {
                catalog,
                schema,
                table,
            } => ResolvedTableReference {
                catalog,
                schema,
                table,
            },
            Self::Partial { schema, table } => ResolvedTableReference {
                catalog: default_catalog.into(),
                schema,
                table,
            },
            Self::Bare { table } => ResolvedTableReference {
                catalog: default_catalog.into(),
                schema: default_schema.into(),
                table,
            },
        }
    }

    /// Forms a string where the identifiers are quoted
    ///
    /// # Example
    /// ```
    /// # use datafusion_common::TableReference;
    /// let table_reference = TableReference::partial("myschema", "mytable");
    /// assert_eq!(table_reference.to_quoted_string(), "myschema.mytable");
    ///
    /// let table_reference = TableReference::partial("MySchema", "MyTable");
    /// assert_eq!(table_reference.to_quoted_string(), r#""MySchema"."MyTable""#);
    /// ```
    pub fn to_quoted_string(&self) -> String {
        match self {
            TableReference::Bare { table } => quote_identifier(table).to_string(),
            TableReference::Partial { schema, table } => {
                format!("{}.{}", quote_identifier(schema), quote_identifier(table))
            }
            TableReference::Full {
                catalog,
                schema,
                table,
            } => format!(
                "{}.{}.{}",
                quote_identifier(catalog),
                quote_identifier(schema),
                quote_identifier(table)
            ),
        }
    }

    /// Forms a [`TableReference`] by parsing `s` as a multipart SQL
    /// identifier. See docs on [`TableReference`] for more details.
    pub fn parse_str(s: &str) -> Self {
        let mut parts = parse_identifiers_normalized(s, false);

        match parts.len() {
            1 => Self::Bare {
                table: parts.remove(0).into(),
            },
            2 => Self::Partial {
                schema: parts.remove(0).into(),
                table: parts.remove(0).into(),
            },
            3 => Self::Full {
                catalog: parts.remove(0).into(),
                schema: parts.remove(0).into(),
                table: parts.remove(0).into(),
            },
            _ => Self::Bare { table: s.into() },
        }
    }

    /// Decompose a [`TableReference`] to separate parts. The result vector contains
    /// at most three elements in the following sequence:
    /// ```no_rust
    /// [<catalog>, <schema>, table]
    /// ```
    pub fn to_vec(&self) -> Vec<String> {
        match self {
            TableReference::Bare { table } => vec![table.to_string()],
            TableReference::Partial { schema, table } => {
                vec![schema.to_string(), table.to_string()]
            }
            TableReference::Full {
                catalog,
                schema,
                table,
            } => vec![catalog.to_string(), schema.to_string(), table.to_string()],
        }
    }
}

/// Parse a string into a TableReference, normalizing where appropriate
///
/// See full details on [`TableReference::parse_str`]
impl<'a> From<&'a str> for TableReference {
    fn from(s: &'a str) -> Self {
        Self::parse_str(s)
    }
}

impl<'a> From<&'a String> for TableReference {
    fn from(s: &'a String) -> Self {
        Self::parse_str(s)
    }
}

impl From<String> for TableReference {
    fn from(s: String) -> Self {
        Self::parse_str(&s)
    }
}

impl From<ResolvedTableReference> for TableReference {
    fn from(resolved: ResolvedTableReference) -> Self {
        Self::Full {
            catalog: resolved.catalog,
            schema: resolved.schema,
            table: resolved.table,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_table_reference_from_str_normalizes() {
        let expected = TableReference::Full {
            catalog: "catalog".into(),
            schema: "FOO\".bar".into(),
            table: "table".into(),
        };
        let actual = TableReference::from("catalog.\"FOO\"\".bar\".TABLE");
        assert_eq!(expected, actual);

        let expected = TableReference::Partial {
            schema: "FOO\".bar".into(),
            table: "table".into(),
        };
        let actual = TableReference::from("\"FOO\"\".bar\".TABLE");
        assert_eq!(expected, actual);

        let expected = TableReference::Bare {
            table: "table".into(),
        };
        let actual = TableReference::from("TABLE");
        assert_eq!(expected, actual);

        // if fail to parse, take entire input string as identifier
        let expected = TableReference::Bare {
            table: "TABLE()".into(),
        };
        let actual = TableReference::from("TABLE()");
        assert_eq!(expected, actual);
    }

    #[test]
    fn test_table_reference_to_vector() {
        let table_reference = TableReference::parse_str("table");
        assert_eq!(vec!["table".to_string()], table_reference.to_vec());

        let table_reference = TableReference::parse_str("schema.table");
        assert_eq!(
            vec!["schema".to_string(), "table".to_string()],
            table_reference.to_vec()
        );

        let table_reference = TableReference::parse_str("catalog.schema.table");
        assert_eq!(
            vec![
                "catalog".to_string(),
                "schema".to_string(),
                "table".to_string()
            ],
            table_reference.to_vec()
        );
    }
}
