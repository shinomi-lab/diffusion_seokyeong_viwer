//! Web 版の計算の窓口。ロジックは持たず、`sim::Sim`（`sim-api` の薄い包み）を wasm-bindgen で公開するだけ。
//! 文字列で返すものは JSON、`Uint*/Float*Array` で返すものは列指向の配列（8.1）。
//! エラーは日本語のメッセージ付きの JS 例外。

pub mod golden;
pub mod sim;

use wasm_bindgen::prelude::*;

fn e(s: String) -> JsValue {
    JsValue::from_str(&s)
}

#[wasm_bindgen(start)]
pub fn init() {
    console_error_panic_hook::set_once();
}

/// 過去のラウンドのデータ（JS 側からは getter で読む）。
#[wasm_bindgen]
pub struct RoundDataJs {
    inner: sim::RoundData,
}

#[wasm_bindgen]
impl RoundDataJs {
    #[wasm_bindgen(getter, js_name = summaryJson)]
    pub fn summary_json(&self) -> String {
        self.inner.summary_json.clone()
    }
    #[wasm_bindgen(getter)]
    pub fn wave(&self) -> Vec<u16> {
        self.inner.events.wave.clone()
    }
    #[wasm_bindgen(getter)]
    pub fn node(&self) -> Vec<u32> {
        self.inner.events.node.clone()
    }
    #[wasm_bindgen(getter)]
    pub fn from(&self) -> Vec<u32> {
        self.inner.events.from.clone()
    }
    #[wasm_bindgen(getter, js_name = nArr)]
    pub fn n_arr(&self) -> Vec<u8> {
        self.inner.events.n_arr.clone()
    }
    #[wasm_bindgen(getter, js_name = uInt)]
    pub fn u_int(&self) -> Vec<f32> {
        self.inner.events.u_int.clone()
    }
    #[wasm_bindgen(getter, js_name = uExt)]
    pub fn u_ext(&self) -> Vec<f32> {
        self.inner.events.u_ext.clone()
    }
    #[wasm_bindgen(getter)]
    pub fn flags(&self) -> Vec<u8> {
        self.inner.events.flags.clone()
    }
    #[wasm_bindgen(getter, js_name = everBefore)]
    pub fn ever_before(&self) -> Vec<u8> {
        self.inner.ever_before.clone()
    }
    #[wasm_bindgen(getter, js_name = everAfter)]
    pub fn ever_after(&self) -> Vec<u8> {
        self.inner.ever_after.clone()
    }
}

#[wasm_bindgen]
pub struct Simulator {
    inner: sim::Sim,
}

#[wasm_bindgen]
impl Simulator {
    #[wasm_bindgen(constructor)]
    pub fn new() -> Simulator {
        Simulator {
            inner: sim::Sim::new(),
        }
    }

    #[wasm_bindgen(js_name = loadGraph)]
    pub fn load_graph(&mut self, bytes: &[u8], opts_json: &str) -> Result<String, JsValue> {
        self.inner.load_graph(bytes, opts_json).map_err(e)
    }

    #[wasm_bindgen(js_name = paramSpecs)]
    pub fn param_specs(&self) -> Result<String, JsValue> {
        self.inner.param_specs().map_err(e)
    }

    #[wasm_bindgen(js_name = createScenario)]
    pub fn create_scenario(&mut self, config_json: &str) -> Result<String, JsValue> {
        self.inner.create_scenario(config_json).map_err(e)
    }

    #[wasm_bindgen(js_name = centralityRanking)]
    pub fn centrality_ranking(
        &self,
        metric: &str,
        top: u32,
        vc_csv: Option<String>,
    ) -> Result<Vec<u32>, JsValue> {
        self.inner
            .centrality_ranking(metric, top as usize, vc_csv.as_deref())
            .map_err(e)
    }

    #[wasm_bindgen(js_name = setIp)]
    pub fn set_ip(&mut self, node: u32) -> Result<String, JsValue> {
        self.inner.set_ip(node).map_err(e)
    }

    #[wasm_bindgen(js_name = ringAngles)]
    pub fn ring_angles(&self) -> Result<Vec<f32>, JsValue> {
        self.inner.ring_angles().map_err(e)
    }

    #[wasm_bindgen(js_name = ringDist)]
    pub fn ring_dist(&self) -> Result<Vec<u32>, JsValue> {
        self.inner.ring_dist().map_err(e)
    }

    pub fn step(&mut self, choice: i32) -> Result<String, JsValue> {
        self.inner.step(choice).map_err(e)
    }

    #[wasm_bindgen(js_name = eventWave)]
    pub fn event_wave(&self) -> Result<Vec<u16>, JsValue> {
        Ok(self.inner.last_events().map_err(e)?.wave.clone())
    }

    #[wasm_bindgen(js_name = eventNode)]
    pub fn event_node(&self) -> Result<Vec<u32>, JsValue> {
        Ok(self.inner.last_events().map_err(e)?.node.clone())
    }

    #[wasm_bindgen(js_name = eventFrom)]
    pub fn event_from(&self) -> Result<Vec<u32>, JsValue> {
        Ok(self.inner.last_events().map_err(e)?.from.clone())
    }

    #[wasm_bindgen(js_name = eventNArr)]
    pub fn event_n_arr(&self) -> Result<Vec<u8>, JsValue> {
        Ok(self.inner.last_events().map_err(e)?.n_arr.clone())
    }

    #[wasm_bindgen(js_name = eventUInt)]
    pub fn event_u_int(&self) -> Result<Vec<f32>, JsValue> {
        Ok(self.inner.last_events().map_err(e)?.u_int.clone())
    }

    #[wasm_bindgen(js_name = eventUExt)]
    pub fn event_u_ext(&self) -> Result<Vec<f32>, JsValue> {
        Ok(self.inner.last_events().map_err(e)?.u_ext.clone())
    }

    #[wasm_bindgen(js_name = eventFlags)]
    pub fn event_flags(&self) -> Result<Vec<u8>, JsValue> {
        Ok(self.inner.last_events().map_err(e)?.flags.clone())
    }

    #[wasm_bindgen(js_name = everFlags)]
    pub fn ever_flags(&self) -> Result<Vec<u8>, JsValue> {
        self.inner.ever_flags().map_err(e)
    }

    pub fn kinds(&self, int_mode: u8) -> Result<Vec<u8>, JsValue> {
        self.inner.kinds(int_mode).map_err(e)
    }

    #[wasm_bindgen(js_name = nodeDetail)]
    pub fn node_detail(&self, node: u32) -> Result<String, JsValue> {
        self.inner.node_detail(node).map_err(e)
    }

    #[wasm_bindgen(js_name = groupSeries)]
    pub fn group_series(
        &self,
        sel: u8,
        at_round: u32,
        int_mode: u8,
        custom_nodes: Option<Vec<u32>>,
    ) -> Result<String, JsValue> {
        self.inner
            .group_series(sel, at_round, int_mode, &custom_nodes.unwrap_or_default())
            .map_err(e)
    }

    #[wasm_bindgen(js_name = sectorGrid)]
    pub fn sector_grid(&self, int_mode: u8) -> Result<Vec<u32>, JsValue> {
        self.inner.sector_grid(int_mode).map_err(e)
    }

    #[wasm_bindgen(js_name = selectNodes)]
    pub fn select_nodes(
        &self,
        ring_lo: u32,
        ring_hi: u32,
        ang_lo: f32,
        ang_hi: f32,
    ) -> Result<Vec<u32>, JsValue> {
        self.inner
            .select_nodes(ring_lo, ring_hi, ang_lo, ang_hi)
            .map_err(e)
    }

    #[wasm_bindgen(js_name = selectUnreachable)]
    pub fn select_unreachable(&self) -> Result<Vec<u32>, JsValue> {
        self.inner.select_unreachable().map_err(e)
    }

    #[wasm_bindgen(js_name = edgesAmong)]
    pub fn edges_among(&self, nodes: Vec<u32>, cap: u32) -> Result<Vec<u32>, JsValue> {
        self.inner.edges_among(&nodes, cap as usize).map_err(e)
    }

    #[wasm_bindgen(js_name = runMc)]
    pub fn run_mc(&self, seq: Vec<i8>, trials: u32, seed: f64) -> Result<String, JsValue> {
        self.inner.run_mc(&seq, trials, seed).map_err(e)
    }

    #[wasm_bindgen(js_name = exportScenarioJson)]
    pub fn export_scenario_json(&self) -> Result<String, JsValue> {
        self.inner.export_scenario_json().map_err(e)
    }

    #[wasm_bindgen(js_name = exportRoundsCsv)]
    pub fn export_rounds_csv(&self) -> Result<String, JsValue> {
        self.inner.export_rounds_csv().map_err(e)
    }

    #[wasm_bindgen(js_name = exportEventsCsv)]
    pub fn export_events_csv(&self, round: Option<u32>) -> Result<String, JsValue> {
        self.inner.export_events_csv(round).map_err(e)
    }

    #[wasm_bindgen(js_name = branchTree)]
    pub fn branch_tree(&self) -> Result<String, JsValue> {
        self.inner.branch_tree().map_err(e)
    }

    #[wasm_bindgen(js_name = rewindTo)]
    pub fn rewind_to(&mut self, round: u32) -> Result<(), JsValue> {
        self.inner.rewind_to(round).map_err(e)
    }

    #[wasm_bindgen(js_name = switchTo)]
    pub fn switch_to(&mut self, id: u32) -> Result<String, JsValue> {
        self.inner.switch_to(id).map_err(e)
    }

    #[wasm_bindgen(js_name = roundData)]
    pub fn round_data(&self, r: u32) -> Result<RoundDataJs, JsValue> {
        self.inner
            .round_data(r)
            .map(|inner| RoundDataJs { inner })
            .map_err(e)
    }

    #[wasm_bindgen(js_name = importRecord)]
    pub fn import_record(&mut self, record_json: &str) -> Result<String, JsValue> {
        self.inner.import_record(record_json).map_err(e)
    }

    pub fn reset(&mut self) -> Result<(), JsValue> {
        self.inner.reset().map_err(e)
    }
}

impl Default for Simulator {
    fn default() -> Self {
        Self::new()
    }
}
