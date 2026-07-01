use anyhow::Result;
use instant_distance::{Builder, HnswMap, Point, Search};
use serde::{Deserialize, Serialize};

pub const HNSW_THRESHOLD: usize = 12_000;

#[derive(Clone, Serialize, Deserialize)]
pub struct EmbPoint(pub Vec<f32>);

impl Point for EmbPoint {
    fn distance(&self, other: &Self) -> f32 {
        let dot: f32 = self.0.iter().zip(other.0.iter()).map(|(a, b)| a * b).sum();
        let na: f32 = self.0.iter().map(|x| x * x).sum::<f32>().sqrt();
        let nb: f32 = other.0.iter().map(|x| x * x).sum::<f32>().sqrt();
        if na == 0.0 || nb == 0.0 {
            return 1.0;
        }
        1.0 - (dot / (na * nb))
    }
}

/// Serializable raw data for HNSW (points + node_ids, without built HnswMap).
#[derive(Serialize, Deserialize)]
pub struct HnswData {
    pub points: Vec<EmbPoint>,
    pub node_ids: Vec<String>,
}

/// Runtime HNSW index wrapping the built HnswMap + original data for serialization.
pub struct HnswIndex {
    pub map: HnswMap<EmbPoint, String>,
    pub node_ids: Vec<String>,
    pub(crate) points: Vec<EmbPoint>,
}

pub fn build_hnsw(entries: Vec<(String, Vec<u8>)>) -> Option<HnswIndex> {
    if entries.len() < HNSW_THRESHOLD {
        return None;
    }
    let node_ids: Vec<String> = entries.iter().map(|(id, _)| id.clone()).collect();
    let points: Vec<EmbPoint> = entries
        .iter()
        .map(|(_, bytes)| EmbPoint(bytes_to_f32(bytes)))
        .collect();
    let values: Vec<String> = node_ids.clone();
    let map = Builder::default().build(points.clone(), values);
    Some(HnswIndex { map, node_ids, points })
}

pub fn search_hnsw(index: &HnswIndex, query_bytes: &[f32], k: usize) -> Vec<(String, f32)> {
    let query_point = EmbPoint(query_bytes.to_vec());
    let mut search = Search::default();
    index
        .map
        .search(&query_point, &mut search)
        .take(k)
        .map(|item| (item.value.clone(), item.distance))
        .collect()
}

/// Serialize raw points + node_ids for storage.
pub fn serialize_hnsw_data(index: &HnswIndex) -> Result<Vec<u8>> {
    let data = HnswData {
        points: index.points.clone(),
        node_ids: index.node_ids.clone(),
    };
    Ok(bincode::serialize(&data)?)
}

/// Deserialize raw data and rebuild the HnswMap.
pub fn deserialize_hnsw_rebuild(bytes: &[u8]) -> Result<HnswIndex> {
    let data: HnswData = bincode::deserialize(bytes)?;
    let values: Vec<String> = data.node_ids.clone();
    let map = Builder::default().build(data.points.clone(), values);
    Ok(HnswIndex {
        map,
        node_ids: data.node_ids,
        points: data.points,
    })
}

fn bytes_to_f32(bytes: &[u8]) -> Vec<f32> {
    bytes
        .chunks_exact(4)
        .map(|c| f32::from_le_bytes([c[0], c[1], c[2], c[3]]))
        .collect()
}
