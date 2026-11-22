use super::{Message, Phrasebook, StepWords};
use crate::settings::Units;
use routing::{AlertKind, ManeuverKind, Modifier};

pub struct Japanese;

impl Japanese {
    fn direction(modifier: Option<Modifier>) -> &'static str {
        match modifier {
            Some(Modifier::UTurn) => "Uターン",
            Some(Modifier::SharpRight) => "大きく右方向",
            Some(Modifier::Right) => "右方向",
            Some(Modifier::SlightRight) => "斜め右方向",
            Some(Modifier::SlightLeft) => "斜め左方向",
            Some(Modifier::Left) => "左方向",
            Some(Modifier::SharpLeft) => "大きく左方向",
            Some(Modifier::Straight) | None => "直進",
        }
    }

    fn side(modifier: Option<Modifier>) -> &'static str {
        if modifier.is_some_and(|m| m.is_left()) { "左" } else { "右" }
    }
}

impl Phrasebook for Japanese {
    fn tag(&self) -> &'static str {
        "ja-JP"
    }

    fn instruction(&self, s: &StepWords) -> String {
        let onto = |text: &str| match s.road {
            Some(road) => format!("{road}へ{text}"),
            None => text.to_string(),
        };
        let toward = s.destinations.map(|d| format!("{}方面、", d.replace(", ", "・"))).unwrap_or_default();
        // 「日比谷交差点を」: the junction where the maneuver is, when it has a name.
        let at = s
            .junction
            .map(|j| {
                if j.ends_with("交差点") || j.ends_with("IC") || j.ends_with("JCT") || j.ends_with("インター") {
                    format!("{j}を")
                } else {
                    format!("{j}交差点を")
                }
            })
            .unwrap_or_default();
        let exit = s.exit_number.map(|e| format!("{e}番出口、")).unwrap_or_default();
        match s.kind {
            ManeuverKind::Depart => match s.road {
                Some(road) => format!("{road}を進みます"),
                None => "出発します".into(),
            },
            ManeuverKind::Arrive if s.last => "目的地に到着です".into(),
            ManeuverKind::Arrive => "経由地に到着です".into(),
            ManeuverKind::Turn => match s.modifier {
                Some(Modifier::UTurn) => "Uターンです".into(),
                Some(Modifier::Straight) | None => onto("直進です"),
                m => format!("{at}{toward}{}", onto(&format!("{}です", Self::direction(m)))),
            },
            ManeuverKind::Continue => onto("直進です"),
            ManeuverKind::NewName => match s.road {
                Some(road) => format!("道なりに{road}を進みます"),
                None => "道なりに進みます".into(),
            },
            ManeuverKind::EndOfRoad => match s.junction {
                Some(_) => format!("突き当たりの{at}{}", onto(&format!("{}です", Self::direction(s.modifier)))),
                None => format!("突き当たりを{}", onto(&format!("{}です", Self::direction(s.modifier)))),
            },
            ManeuverKind::Fork => format!("{at}{toward}{}", onto(&format!("{}側の道を進みます", Self::side(s.modifier)))),
            ManeuverKind::OnRamp => format!("{toward}{}", onto(&format!("{}方向の入口に入ります", Self::side(s.modifier)))),
            ManeuverKind::OffRamp => format!("{exit}{toward}{}", onto(&format!("{}方向の出口です", Self::side(s.modifier)))),
            ManeuverKind::Merge => match s.road {
                Some(road) => format!("{road}に合流します"),
                None => "本線に合流します".into(),
            },
            ManeuverKind::Roundabout => match (s.exit, s.road) {
                (Some(n), Some(road)) => format!("ロータリーで{n}番目の出口から{road}へ出ます"),
                (Some(n), None) => format!("ロータリーで{n}番目の出口を出ます"),
                _ => "ロータリーに入ります".into(),
            },
            ManeuverKind::ExitRoundabout => onto("ロータリーを出ます"),
            ManeuverKind::Ferry => "フェリーに乗ります".into(),
            ManeuverKind::Transit => s.instruction.map(str::to_string).unwrap_or_else(|| "公共交通機関に乗ります".into()),
        }
    }

    fn prepare(&self, distance: &str, instruction: &str) -> String {
        format!("{distance}先、{instruction}")
    }

    fn now(&self, instruction: &str, then: Option<&str>) -> String {
        match then {
            Some(then) => format!("まもなく、{instruction}。その先、{then}"),
            None => format!("まもなく、{instruction}"),
        }
    }

    fn spoken_distance(&self, metres: f64, _units: Units, imperial: bool) -> String {
        if imperial {
            let miles = metres / 1609.344;
            return if miles < 0.1 {
                format!("{}フィート", ((metres * 3.28084 / 50.0).round() * 50.0).max(50.0) as u64)
            } else {
                format!("{miles:.1}マイル")
            };
        }
        if metres < 1000.0 {
            let rounded = if metres < 100.0 { (metres / 10.0).round() * 10.0 } else { (metres / 50.0).round() * 50.0 };
            format!("{}メートル", rounded.max(10.0) as u64)
        } else {
            let km = metres / 1000.0;
            if (km - km.round()).abs() < 0.05 { format!("{}キロ", km.round() as u64) } else { format!("{km:.1}キロ") }
        }
    }

    fn duration(&self, seconds: f64) -> String {
        let minutes = (seconds / 60.0).round().max(1.0) as u64;
        match (minutes / 60, minutes % 60) {
            (0, m) => format!("{m} 分"),
            (h, 0) => format!("{h} 時間"),
            (h, m) => format!("{h} 時間 {m} 分"),
        }
    }

    fn message(&self, message: Message) -> &'static str {
        match message {
            Message::NoRoute => "経路が見つかりません",
            Message::NoRoad => "出発地か目的地の近くに道路がありません",
            Message::OutOfCoverage => "ダウンロードした地域の外です",
            Message::UnsupportedMode => "この移動手段には対応していません",
            Message::NetworkError => "ネットワークに接続できません",
            Message::ServerError => "サーバーが応答しませんでした",
            Message::NoRouter => "使えるルーターがありません",
            Message::NoResults => "見つかりませんでした",
            Message::SearchFailed => "検索できませんでした",
            Message::RouteFrom => "出発地",
            Message::MyLocation => "現在地",
            Message::LocationUnknown => "現在地がわかりません",
            Message::Locating => "現在地を取得しています…",
            Message::Rerouting => "経路を再検索しています",
            Message::Arrived => "目的地に到着しました",
            Message::StartGuidance => "案内を開始します",
            Message::TollsNotAvoided => "有料道路を避けられませんでした",
            Message::HighwaysNotAvoided => "高速道路を避けられませんでした",
            Message::FerriesNotAvoided => "フェリーを避けられませんでした",
            Message::GoogleRefused => "Google がリクエストを拒否しました",
            Message::RegionTooLarge => "範囲が広すぎます。地図を拡大してください",
            Message::RegionDownloading => "ダウンロード中",
            Message::RegionBuilding => "経路データを作成中",
            Message::RegionReady => "利用可能",
            Message::RegionFailed => "失敗しました",
            Message::DroppedPin => "ドロップしたピン",
            Message::Home => "自宅",
            Message::Work => "職場",
            Message::UnpavedNotAvoided => "未舗装路を避けられなかった可能性があります",
            Message::StepsNotAvoided => "階段を避けられなかった可能性があります",
            Message::LevelCrossingsNotAvoided => "踏切を避けられなかった可能性があります",
            Message::AvoidedAreaEntered => "避けるエリアを通ります",
            Message::RegionElevation => "標高データをダウンロード中",
            Message::FasterRouteTaken => "より速いルートに変更しました",
            Message::Estimated => "GPS 信号なし（推定位置）",
            Message::TrafficFailed => "交通情報を取得できませんでした",
        }
    }

    fn via(&self, roads: &str) -> String {
        format!("{roads} 経由")
    }

    fn fallback(&self, used: &str, reason: &str) -> String {
        format!("{used} を使用しました（{reason}）")
    }

    fn ordinal_exit(&self, exit: u32) -> String {
        format!("{exit}番目の出口")
    }

    fn alert_name(&self, kind: AlertKind) -> &'static str {
        match kind {
            AlertKind::LevelCrossing => "踏切",
            AlertKind::SpeedCamera => "速度取締機",
            AlertKind::Stop => "一時停止",
            AlertKind::TollBooth => "料金所",
            AlertKind::Steep => "急な上り坂",
            AlertKind::Incident => "事故・通行止め",
        }
    }

    fn alert(&self, kind: AlertKind, distance: &str) -> String {
        format!("{distance}先、{}があります", self.alert_name(kind))
    }

    fn faster_route(&self, saving: &str) -> String {
        format!("{saving}早いルートがあります")
    }

    fn congestion(&self, distance: &str, length: &str) -> String {
        format!("{distance}先から{length}の渋滞")
    }

    fn destinations(&self, list: &str) -> String {
        list.replace(", ", "・")
    }
}
