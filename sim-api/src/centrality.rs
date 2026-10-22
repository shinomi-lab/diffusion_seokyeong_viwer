//! IP 候補の順位（7.11）。VC はアプリ内では計算せず、事前計算の CSV から読む（D14）。

use crate::csr::Csr;

/// 次数（フォロワー数）= 拡散グラフの出次数。
pub fn out_degree(csr: &Csr) -> Vec<u32> {
    (0..csr.n_nodes() as u32)
        .map(|i| csr.out_degree(i) as u32)
        .collect()
}

/// 拡散グラフの**逆向き**（`predecessors` をたどる）の PageRank。減衰 0.85、50 反復、行き止まりは一様に分配。
/// 入力ファイルの向きでの PageRank（過去の Python 実装 `reverse_edges=False`）に一致させる（D15）。
pub fn pagerank_reverse(csr: &Csr) -> Vec<f64> {
    let n = csr.n_nodes();
    let d = 0.85;
    let nf = n as f64;
    let mut r = vec![1.0 / nf; n];
    let mut next = vec![0.0; n];
    for _ in 0..50 {
        // 逆向きグラフでの v の出次数 = 拡散グラフでの入次数。v は先行 u へ r[v]/indeg(v) を渡す
        let dangling: f64 = (0..n)
            .filter(|&v| csr.pred(v as u32).is_empty())
            .map(|v| r[v])
            .sum();
        next.fill((1.0 - d) / nf + d * dangling / nf);
        for (v, &rv) in r.iter().enumerate() {
            let p = csr.pred(v as u32);
            if p.is_empty() {
                continue;
            }
            let share = d * rv / p.len() as f64;
            for &u in p {
                next[u as usize] += share;
            }
        }
        std::mem::swap(&mut r, &mut next);
    }
    r
}

/// 値の大きい順のノード番号。同値は番号の昇順（決定的）。
pub fn ranking(scores: &[f64], top: usize) -> Vec<u32> {
    let mut idx: Vec<u32> = (0..scores.len() as u32).collect();
    idx.sort_by(|&a, &b| {
        scores[b as usize]
            .total_cmp(&scores[a as usize])
            .then(a.cmp(&b))
    });
    idx.truncate(top);
    idx
}

/// 事前計算 CSV（`node, degree, pagerank, vc`。`node` は元のラベル）から 1 列を読む。
/// 先頭行が数値でなければヘッダーとして列名を探す。CSV に無いノードのスコアは `NEG_INFINITY`。
// 仮定: Q9。区切りはカンマ、ヘッダーあり/なし両対応、無ければ列順は node,degree,pagerank,vc。
pub fn metric_from_csv(text: &str, labels: &[i64], column: &str) -> Result<Vec<f64>, String> {
    let mut col = ["node", "degree", "pagerank", "vc"]
        .iter()
        .position(|&c| c == column)
        .ok_or_else(|| format!("未知の列名です: {column}"))?;
    let mut scores = vec![f64::NEG_INFINITY; labels.len()];
    let mut first = true;
    for (i, line) in text.lines().enumerate() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let cells: Vec<&str> = line.split(',').map(str::trim).collect();
        if first {
            first = false;
            if cells[0].parse::<i64>().is_err() {
                col = cells
                    .iter()
                    .position(|c| c.eq_ignore_ascii_case(column))
                    .ok_or_else(|| format!("列 {column} がヘッダーにありません"))?;
                continue;
            }
        }
        let bad = || format!("{} 行目を解析できません: {line}", i + 1);
        let label: i64 = cells.first().ok_or_else(bad)?.parse().map_err(|_| bad())?;
        let v: f64 = cells.get(col).ok_or_else(bad)?.parse().map_err(|_| bad())?;
        if let Ok(k) = labels.binary_search(&label) {
            scores[k] = v;
        }
    }
    Ok(scores)
}
