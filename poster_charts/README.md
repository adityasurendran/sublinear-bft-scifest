# Sublyne SciFest Poster Charts

**Created**: 2026-02-27  
**Resolution**: 300 DPI (print-ready)  
**Format**: PNG  
**Total Size**: 1.6 MB

---

## Chart Inventory

| # | Filename | Purpose | Size |
|:---:|:---|:---|:---:|
| 1 | `01_scaling_comparison.png` | O(1) vs O(n) certificate sizes | 202 KB |
| 2 | `02_throughput_scaling.png` | 27% throughput cost (dual-axis) | 246 KB |
| 3 | `03_global_latency.png` | 280ms max latency table | 167 KB |
| 4 | `04_byzantine_resilience.png` | Fault recovery timeline | 210 KB |
| 5 | `05_ml_batching.png` | ML adaptive batching (dual-axis) | 212 KB |
| 6 | `06_competitive_comparison.png` | vs. PBFT/Tendermint/etc. | 160 KB |
| 7 | `07_hardware_acceleration.png` | 18× speedup (x86 vs ARM) | 133 KB |
| 8 | `08_real_world_impact.png` | $25B/year savings table | 197 KB |

---

## Color Scheme

- **Primary**: Blue (#3B82F6) - Sublyne brand
- **Secondary**: Purple (#8B5CF6) - Highlights
- **Accent**: Green (#10B981) - Positive metrics
- **Warning**: Orange (#F59E0B) - Degradation
- **Danger**: Red (#EF4444) - Problems/competitors

---

## Usage

### For Poster Board
1. Print at **300 DPI** on glossy photo paper
2. Use **36" × 48" tri-fold board**
3. Matte finish to reduce glare
4. Recommended layout: See `POSTER_GRAPHICS_DATA.md`

### For Digital Presentation
- Use directly in PowerPoint/Keynote
- Already optimized for large displays
- High contrast for projector visibility

---

## Regeneration

To recreate charts from data:
```bash
# All chart data is in POSTER_GRAPHICS_DATA.md
# Charts were generated with matplotlib
python3 generate_charts.py
```

---

**Generated with**: matplotlib 3.x  
**Data source**: `POSTER_GRAPHICS_DATA.md`
