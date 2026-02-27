// Multi-Region Geo-Latency Simulator
// Based on real AWS inter-region ping data (2024-2026)

use std::collections::HashMap;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Region {
    USEast,      // N. Virginia
    USWest,      // Oregon
    EUWest,      // Ireland
    EUCentral,   // Frankfurt
    APSouth,     // Mumbai
    APNortheast, // Tokyo
    SAEast,      // São Paulo
    AFSouth,     // Cape Town
}

pub struct GeoLatencyMatrix {
    matrix: HashMap<(Region, Region), u64>, // Latency in microseconds
}

impl GeoLatencyMatrix {
    pub fn new() -> Self {
        let mut matrix = HashMap::new();
        
        // Intra-region latency (same datacenter cluster)
        for region in Self::all_regions() {
            matrix.insert((region, region), 1_000); // 1ms
        }
        
        // USA ↔ USA
        matrix.insert((Region::USEast, Region::USWest), 60_000); // 60ms
        matrix.insert((Region::USWest, Region::USEast), 60_000);
        
        // USA ↔ Europe
        matrix.insert((Region::USEast, Region::EUWest), 75_000); // 75ms
        matrix.insert((Region::USEast, Region::EUCentral), 85_000);
        matrix.insert((Region::USWest, Region::EUWest), 140_000);
        matrix.insert((Region::USWest, Region::EUCentral), 150_000);
        
        // USA ↔ Asia
        matrix.insert((Region::USEast, Region::APSouth), 200_000); // 200ms
        matrix.insert((Region::USEast, Region::APNortheast), 150_000);
        matrix.insert((Region::USWest, Region::APSouth), 180_000);
        matrix.insert((Region::USWest, Region::APNortheast), 100_000); // 100ms (Pacific)
        
        // USA ↔ South America
        matrix.insert((Region::USEast, Region::SAEast), 110_000);
        matrix.insert((Region::USWest, Region::SAEast), 180_000);
        
        // USA ↔ Africa
        matrix.insert((Region::USEast, Region::AFSouth), 210_000);
        matrix.insert((Region::USWest, Region::AFSouth), 280_000);
        
        // Europe ↔ Europe
        matrix.insert((Region::EUWest, Region::EUCentral), 15_000); // 15ms
        matrix.insert((Region::EUCentral, Region::EUWest), 15_000);
        
        // Europe ↔ Asia
        matrix.insert((Region::EUWest, Region::APSouth), 120_000);
        matrix.insert((Region::EUWest, Region::APNortheast), 220_000);
        matrix.insert((Region::EUCentral, Region::APSouth), 110_000);
        matrix.insert((Region::EUCentral, Region::APNortheast), 210_000);
        
        // Europe ↔ South America
        matrix.insert((Region::EUWest, Region::SAEast), 180_000);
        matrix.insert((Region::EUCentral, Region::SAEast), 190_000);
        
        // Europe ↔ Africa
        matrix.insert((Region::EUWest, Region::AFSouth), 160_000);
        matrix.insert((Region::EUCentral, Region::AFSouth), 150_000);
        
        // Asia ↔ Asia
        matrix.insert((Region::APSouth, Region::APNortheast), 90_000); // 90ms
        matrix.insert((Region::APNortheast, Region::APSouth), 90_000);
        
        // Asia ↔ South America
        matrix.insert((Region::APSouth, Region::SAEast), 310_000); // 310ms (extreme)
        matrix.insert((Region::APNortheast, Region::SAEast), 280_000);
        
        // Asia ↔ Africa
        matrix.insert((Region::APSouth, Region::AFSouth), 140_000);
        matrix.insert((Region::APNortheast, Region::AFSouth), 250_000);
        
        // South America ↔ Africa
        matrix.insert((Region::SAEast, Region::AFSouth), 270_000);
        
        // Fill symmetric entries
        let entries: Vec<_> = matrix.iter().map(|(&k, &v)| (k, v)).collect();
        for ((a, b), latency) in entries {
            matrix.entry((b, a)).or_insert(latency);
        }
        
        Self { matrix }
    }
    
    pub fn get_latency_us(&self, from: Region, to: Region) -> u64 {
        *self.matrix.get(&(from, to)).unwrap_or(&100_000) // Default: 100ms
    }
    
    pub fn get_latency_ms(&self, from: Region, to: Region) -> u64 {
        self.get_latency_us(from, to) / 1000
    }
    
    fn all_regions() -> Vec<Region> {
        vec![
            Region::USEast,
            Region::USWest,
            Region::EUWest,
            Region::EUCentral,
            Region::APSouth,
            Region::APNortheast,
            Region::SAEast,
            Region::AFSouth,
        ]
    }
    
    /// Returns the average latency for a validator set distributed across regions
    pub fn average_latency(&self, regions: &[Region]) -> f64 {
        if regions.len() < 2 {
            return 0.0;
        }
        
        let mut total = 0u64;
        let mut count = 0u64;
        
        for (i, &r1) in regions.iter().enumerate() {
            for &r2 in &regions[i+1..] {
                total += self.get_latency_us(r1, r2);
                count += 1;
            }
        }
        
        (total as f64) / (count as f64) / 1000.0 // Return in ms
    }
    
    /// Maximum latency in the validator set (critical path)
    pub fn max_latency(&self, regions: &[Region]) -> u64 {
        let mut max = 0u64;
        for &r1 in regions {
            for &r2 in regions {
                max = max.max(self.get_latency_us(r1, r2));
            }
        }
        max / 1000 // Return in ms
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_same_region() {
        let matrix = GeoLatencyMatrix::new();
        assert_eq!(matrix.get_latency_ms(Region::USEast, Region::USEast), 1);
    }
    
    #[test]
    fn test_cross_continent() {
        let matrix = GeoLatencyMatrix::new();
        // São Paulo ↔ Tokyo should be ~280ms (extreme distance)
        assert!(matrix.get_latency_ms(Region::SAEast, Region::APNortheast) >= 250);
    }
    
    #[test]
    fn test_symmetry() {
        let matrix = GeoLatencyMatrix::new();
        assert_eq!(
            matrix.get_latency_ms(Region::USEast, Region::EUWest),
            matrix.get_latency_ms(Region::EUWest, Region::USEast)
        );
    }
    
    #[test]
    fn test_global_average() {
        let matrix = GeoLatencyMatrix::new();
        let global_set = vec![
            Region::USEast,
            Region::EUWest,
            Region::APNortheast,
            Region::SAEast,
        ];
        let avg = matrix.average_latency(&global_set);
        println!("Global 4-validator average latency: {:.1}ms", avg);
        assert!(avg > 50.0 && avg < 200.0); // Should be between 50-200ms
    }
}
