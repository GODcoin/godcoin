use prometheus::{default_registry, Histogram, HistogramOpts, HistogramVec, IntCounter, Opts};

lazy_static::lazy_static! {
    pub static ref NET_BYTES_SENT: IntCounter =
        IntCounter::new("net_sent_bytes", "Total network bytes sent").unwrap();
    pub static ref NET_BYTES_RECEIVED: IntCounter =
        IntCounter::new("net_received_bytes", "Total network bytes received").unwrap();

    pub static ref REQ_BROADCAST_FAIL: IntCounter =
        IntCounter::new("req_broadcast_failure", "Total transaction broadcasts rejected").unwrap();
    pub static ref REQ_BROADCAST_TOTAL: IntCounter =
        IntCounter::new("req_broadcast_total", "Total transactions broadcasted").unwrap();

    static ref REQ_DUR: HistogramVec = {
        let opts = Opts::new(
            "request_duration_seconds",
            "Time taken for a request to be processed"
        );
        let mut opts = HistogramOpts::from(opts);
        opts.buckets = vec![
            0.001000, 0.001500, 0.002250, 0.003375, 0.005062, 0.007593, 0.011389, 0.017083,
            0.025624, 0.038436, 0.057654, 0.086481, 0.129721, 0.194581, 0.291871, 0.437806,
            0.656709, 0.985063, 1.477594, 2.21639
        ];
        HistogramVec::new(opts, &["type"]).unwrap()
    };

    pub static ref REQ_BROADCAST_DUR: Histogram = REQ_DUR.with_label_values(&["broadcast"]);
    pub static ref REQ_SET_BLOCK_FILTER_DUR: Histogram = REQ_DUR.with_label_values(
        &["set_block_filter"]
    );
    pub static ref REQ_CLEAR_BLOCK_FILTER_DUR: Histogram = REQ_DUR.with_label_values(
        &["clear_block_filter"]
    );
    pub static ref REQ_SUBSCRIBE_DUR: Histogram = REQ_DUR.with_label_values(&["subscribe"]);
    pub static ref REQ_UNSUBSCRIBE_DUR: Histogram = REQ_DUR.with_label_values(&["unsubscribe"]);
    pub static ref REQ_GET_PROPERTIES_DUR: Histogram = REQ_DUR.with_label_values(
        &["get_properties"]
    );
    pub static ref REQ_GET_BLOCK_DUR: Histogram = REQ_DUR.with_label_values(&["get_block"]);
    pub static ref REQ_GET_FULL_BLOCK_DUR: Histogram = REQ_DUR.with_label_values(
        &["get_full_block"]
    );
    pub static ref REQ_GET_BLOCK_RANGE_DUR: Histogram = REQ_DUR.with_label_values(
        &["get_block_range"]
    );
    pub static ref REQ_GET_ACC_INFO_DUR: Histogram = REQ_DUR.with_label_values(
        &["get_account_info"]
    );
}

pub fn register_metrics() {
    let registry = default_registry();

    macro_rules! register {
        ($metric:expr) => {
            registry.register(Box::new($metric.clone())).unwrap();
        };
    }

    register!(NET_BYTES_SENT);
    register!(NET_BYTES_RECEIVED);

    register!(REQ_BROADCAST_FAIL);
    register!(REQ_BROADCAST_TOTAL);

    register!(REQ_DUR);
    lazy_static::initialize(&REQ_BROADCAST_DUR);
    lazy_static::initialize(&REQ_SET_BLOCK_FILTER_DUR);
    lazy_static::initialize(&REQ_CLEAR_BLOCK_FILTER_DUR);
    lazy_static::initialize(&REQ_SUBSCRIBE_DUR);
    lazy_static::initialize(&REQ_UNSUBSCRIBE_DUR);
    lazy_static::initialize(&REQ_GET_PROPERTIES_DUR);
    lazy_static::initialize(&REQ_GET_BLOCK_DUR);
    lazy_static::initialize(&REQ_GET_FULL_BLOCK_DUR);
    lazy_static::initialize(&REQ_GET_BLOCK_RANGE_DUR);
    lazy_static::initialize(&REQ_GET_ACC_INFO_DUR);
}
