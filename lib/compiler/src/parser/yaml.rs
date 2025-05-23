use anyhow::{anyhow, Result};
use serde_yaml::{
    value::{Tag, TaggedValue},
    Mapping, Value,
};

use std::{
    collections::HashSet,
    fmt,
    fs::{canonicalize, read_to_string, File},
    path::PathBuf,
};

fn load_yaml(file_path: PathBuf) -> Result<Value> {
    let file_reader = File::open(file_path).expect("Unable to open file");
    let data: Value = serde_yaml::from_reader(file_reader)?;

    Ok(data)
}

#[derive(Debug, Clone)]
pub struct Transformer {
    error_on_circular: bool,
    root_path: PathBuf,
    seen_paths: HashSet<PathBuf>, // for circular reference detection
}

impl Transformer {

    pub fn new(root_path: PathBuf, strict: bool) -> Result<Self> {
        Self::new_node(root_path, strict, None)
    }

    pub fn parse(&self) -> Value {
        let file_path = self.root_path.clone();
        let input = load_yaml(file_path).unwrap();

        self.clone().recursive_process(input)
    }

    fn new_node(
        root_path: PathBuf,
        strict: bool,
        seen_paths_option: Option<HashSet<PathBuf>>,
    ) -> Result<Self> {
        let mut seen_paths = match seen_paths_option {
            Some(set) => set,
            None => HashSet::new(),
        };

        let normalized_path = canonicalize(&root_path).unwrap();

        // Circular reference guard
        if seen_paths.contains(&normalized_path) {
            return Err(anyhow!(
                "circular reference: {}",
                &normalized_path.display()
            ));
        }

        seen_paths.insert(normalized_path);

        Ok(Transformer {
            error_on_circular: strict,
            root_path,
            seen_paths,
        })
    }

    fn recursive_process(self, input: Value) -> Value {
        match input {
            Value::Sequence(seq) => seq
                .iter()
                .map(|v| self.clone().recursive_process(v.clone()))
                .collect(),
            Value::Mapping(map) => Value::Mapping(Mapping::from_iter(
                map.iter()
                    .map(|(k, v)| (k.clone(), self.clone().recursive_process(v.clone()))),
            )),
            Value::Tagged(tagged_value) => match tagged_value.tag.to_string().as_str() {
                "!include" => {
                    let value = tagged_value.value.as_str().unwrap();
                    let file_path = PathBuf::from(value);

                    self.handle_include_extension(file_path)
                }
                _ => Value::Tagged(tagged_value),
            },
            // default no transform
            _ => input,
        }
    }

    fn handle_include_extension(&self, file_path: PathBuf) -> Value {
        let normalized_file_path = self.process_path(&file_path);

        let result = match normalized_file_path.extension() {
            Some(os_str) => match os_str.to_str() {
                Some("yaml") | Some("yml") | Some("json") => {
                    match Transformer::new_node(
                        normalized_file_path,
                        self.error_on_circular,
                        Some(self.seen_paths.clone()),
                    ) {
                        Ok(transformer) => transformer.parse(),
                        Err(e) => {
                            if self.error_on_circular {
                                // TODO: probably something better to do than panic ?
                                panic!("{:?}", e);
                            }

                            return Value::Tagged(
                                TaggedValue {
                                    tag: Tag::new("circular"),
                                    value: Value::String(file_path.display().to_string()),
                                }
                                .into(),
                            );
                        }
                    }
                }
                // inlining markdow and text files
                Some("txt") | Some("markdown") | Some("md") => {
                    Value::String(read_to_string(normalized_file_path).unwrap())
                },
                None | Some(&_) => todo!(),
            },
            _ => panic!("{:?} path missing file extension", normalized_file_path),
        };

        result
    }

    fn process_path(&self, file_path: &PathBuf) -> PathBuf {
        if file_path.is_absolute() {
            return file_path.clone();
        }
        let joined = self.root_path.parent().unwrap().join(file_path);

        if !joined.is_file() {
            panic!("{:?} not found", joined);
        }

        canonicalize(joined).unwrap()
    }
}

impl fmt::Display for Transformer {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(
            f,
            "{}",
            serde_yaml::to_string(&self.clone().parse()).unwrap()
        )
    }
}
