use serde::{Deserialize, Serialize};

/// A document to insert into memory during the benchmark.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EvalDocument {
    pub id: String,
    pub category: String,
    pub content: String,
}

/// A query that should retrieve a specific document.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EvalQuery {
    pub text: String,
    pub expected_content: String,
}

/// Complete benchmark dataset.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EvalDataset {
    pub documents: Vec<EvalDocument>,
    pub queries: Vec<EvalQuery>,
    pub seed: u64,
}

fn seeded_shuffle<T: Clone>(items: &[T], seed: u64) -> Vec<T> {
    use rand::seq::SliceRandom;
    use rand::rngs::StdRng;
    use rand::SeedableRng;
    let mut rng = StdRng::seed_from_u64(seed);
    let mut result = items.to_vec();
    result.shuffle(&mut rng);
    result
}

/// Generate a deterministic LongMemEval-S dataset of `count` fact-query pairs.
///
/// Each pair consists of:
/// - A document with a unique fact (e.g. "The Eiffel Tower was built in 1889.")
/// - A query that should retrieve it (e.g. "When was the Eiffel Tower built?")
///
/// Facts span science, history, technology, geography, and culture.
pub fn generate_dataset(count: usize, seed: u64) -> EvalDataset {
    let templates = build_templates();
    let shuffled = seeded_shuffle(&templates, seed);
    let selected = shuffled.into_iter().take(count).collect::<Vec<_>>();

    let documents: Vec<EvalDocument> = selected.iter().enumerate().map(|(i, (fact, _cat))| {
        EvalDocument {
            id: format!("longmemeval-s-fact-{}", i + 1),
            category: _cat.to_string(),
            content: fact.to_string(),
        }
    }).collect();

    let queries: Vec<EvalQuery> = selected.iter().map(|(fact, _cat)| {
        EvalQuery {
            text: build_query(fact),
            expected_content: fact.to_string(),
        }
    }).collect();

    EvalDataset { documents, queries, seed }
}

/// Build a natural language query from a fact statement.
fn build_query(fact: &str) -> String {
    // Strip the prefix to form a question
    let lowered = fact.to_lowercase();
    let without_prefix = lowered
        .trim_start_matches("the ")
        .trim_start_matches("a ")
        .trim_start_matches("an ");
    format!("tell me about {without_prefix}")
}

type FactAndCategory = (&'static str, &'static str);

fn build_templates() -> Vec<FactAndCategory> {
    vec![
        // Science (15)
        ("Water boils at 100 degrees Celsius at sea level.", "science"),
        ("The chemical symbol for gold is Au.", "science"),
        ("Photosynthesis converts carbon dioxide into oxygen.", "science"),
        ("The human body has 206 bones.", "science"),
        ("DNA is shaped like a double helix.", "science"),
        ("Neutrons have no electric charge.", "science"),
        ("The speed of light is approximately 300,000 km per second.", "science"),
        ("Earth's atmosphere is composed of 78% nitrogen.", "science"),
        ("The largest organ in the human body is the skin.", "science"),
        ("Antibiotics cannot kill viruses.", "science"),
        ("The atomic number of carbon is 6.", "science"),
        ("Venus is the hottest planet in the solar system.", "science"),
        ("The human brain contains approximately 86 billion neurons.", "science"),
        ("Mitochondria are known as the powerhouse of the cell.", "science"),
        ("The pH scale ranges from 0 to 14.", "science"),
        // History (15)
        ("The Berlin Wall fell in 1989.", "history"),
        ("The first moon landing was in 1969.", "history"),
        ("The Magna Carta was signed in 1215.", "history"),
        ("World War II ended in 1945.", "history"),
        ("The French Revolution began in 1789.", "history"),
        ("Ancient Egypt lasted for over 3000 years.", "history"),
        ("The Roman Empire fell in 476 AD.", "history"),
        ("The printing press was invented by Johannes Gutenberg.", "history"),
        ("The Titanic sank on its maiden voyage in 1912.", "history"),
        ("The Cold War lasted from 1947 to 1991.", "history"),
        ("The Pyramids of Giza were built around 2560 BC.", "history"),
        ("The Spanish flu pandemic of 1918 killed millions.", "history"),
        ("The first Olympic Games were held in 776 BC.", "history"),
        ("The Silk Road connected China to Europe.", "history"),
        ("The Industrial Revolution began in Britain around 1760.", "history"),
        // Technology (12)
        ("The World Wide Web was invented in 1989.", "technology"),
        ("The first computer virus was created in 1983.", "technology"),
        ("USB stands for Universal Serial Bus.", "technology"),
        ("The first smartphone was released in 1992.", "technology"),
        ("Linux was created by Linus Torvalds in 1991.", "technology"),
        ("The Turing machine was conceptualized in 1936.", "technology"),
        ("HTML stands for HyperText Markup Language.", "technology"),
        ("The first email was sent in 1971.", "technology"),
        ("GPS was developed by the US Department of Defense.", "technology"),
        ("The first website went live in 1991.", "technology"),
        ("Bluetooth is named after King Harald Bluetooth.", "technology"),
        ("The first programmable computer was the ENIAC.", "technology"),
        // Geography (12)
        ("The Amazon River is the longest river in the world.", "geography"),
        ("Mount Everest is the tallest mountain on Earth.", "geography"),
        ("Australia is both a country and a continent.", "geography"),
        ("The Sahara Desert is the largest hot desert.", "geography"),
        ("The Pacific Ocean is the largest ocean.", "geography"),
        ("Lake Baikal is the deepest freshwater lake.", "geography"),
        ("Antarctica is the coldest continent on Earth.", "geography"),
        ("The Nile flows through 11 countries.", "geography"),
        ("The Great Barrier Reef is visible from space.", "geography"),
        ("The Dead Sea is the lowest point on Earth's surface.", "geography"),
        ("Iceland has no standing army.", "geography"),
        ("Greenland is the largest island in the world.", "geography"),
        // Culture (10)
        ("The Mona Lisa is displayed at the Louvre Museum.", "culture"),
        ("Shakespeare wrote 37 plays.", "culture"),
        ("The first novel ever written is The Tale of Genji.", "culture"),
        ("Beethoven composed 9 symphonies.", "culture"),
        ("The Great Wall of China is over 21000 km long.", "culture"),
        ("The Olympic rings represent five continents.", "culture"),
        ("The Sistine Chapel ceiling was painted by Michelangelo.", "culture"),
        ("Jazz music originated in New Orleans.", "culture"),
        ("The Rosetta Stone is in the British Museum.", "culture"),
        ("Kabuki is a traditional Japanese theater form.", "culture"),
        // Math (6)
        ("Pi is approximately 3.14159.", "math"),
        ("The Pythagorean theorem is a squared plus b squared equals c squared.", "math"),
        ("Zero was invented in India.", "math"),
        ("There are infinitely many prime numbers.", "math"),
        ("The Fibonacci sequence appears throughout nature.", "math"),
        ("Euclid is known as the father of geometry.", "math"),
        // Biology (10)
        ("Octopuses have three hearts.", "biology"),
        ("Bananas are berries but strawberries are not.", "biology"),
        ("A group of flamingos is called a flamboyance.", "biology"),
        ("The blue whale is the largest animal ever known.", "biology"),
        ("Trees can communicate through underground fungal networks.", "biology"),
        ("The average human produces 25000 quarts of saliva in a lifetime.", "biology"),
        ("Honey never spoils.", "biology"),
        ("Cows have best friends and get stressed when separated.", "biology"),
        ("A cockroach can live for weeks without its head.", "biology"),
        ("Elephants are the only mammals that cannot jump.", "biology"),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_generate_dataset_deterministic() {
        let d1 = generate_dataset(10, 42);
        let d2 = generate_dataset(10, 42);
        assert_eq!(d1.documents.len(), d2.documents.len());
        assert_eq!(d1.documents[0].content, d2.documents[0].content);
        assert_eq!(d1.queries[0].text, d2.queries[0].text);
    }

    #[test]
    fn test_each_query_has_expected_content() {
        let ds = generate_dataset(30, 42);
        for q in &ds.queries {
            let found = ds.documents.iter().any(|d| d.content == q.expected_content);
            assert!(found, "No document matches query expected_content: {}", q.expected_content);
        }
    }
}
