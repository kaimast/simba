use crate::logic::AccountId;
use crate::logic::Transaction;
use crate::node::{get_node_logic, Node};
use crate::object::{Object, ObjectId};

use std::cell::RefCell;
use std::rc::Rc;
use std::sync::atomic::{AtomicU64, Ordering};

use asim::sync::Notify;
use asim::time::{Duration, Time};

pub struct Client {
    identifier: ObjectId,
    account_id: AccountId,
    start_delay: Duration,
    transaction_interval: Duration,
    node: Rc<Node>,
    next_nonce: AtomicU64,
    txn_issue_time: RefCell<Option<Time>>,
    latencies: RefCell<Vec<Duration>>,
    commit_notify: Notify,
}

impl Client {
    pub(super) fn new(
        start_delay: Duration,
        transaction_interval: Duration,
        node: Rc<Node>,
    ) -> Self {
        let identifier = ObjectId::random();
        let account_id = rand::random::<u128>();
        let txn_issue_time = RefCell::new(None);
        let latencies = RefCell::new(vec![]);
        let commit_notify = Notify::new();
        let next_nonce = AtomicU64::new(1);

        Self {
            identifier,
            account_id,
            txn_issue_time,
            next_nonce,
            start_delay,
            transaction_interval,
            node,
            latencies,
            commit_notify,
        }
    }

    pub(crate) async fn run(&self) {
        if !self.start_delay.is_zero() {
            asim::time::sleep(self.start_delay).await;
        }

        loop {
            log::trace!("Issuing next transaction");

            {
                let mut issue_time = self.txn_issue_time.borrow_mut();
                *issue_time = Some(asim::time::now());
            }

            let nonce = self.next_nonce.fetch_add(1, Ordering::SeqCst);
            let transaction = Transaction::new(self.account_id, nonce);

            get_node_logic(&self.node).add_transaction(
                &self.node,
                Rc::new(transaction),
                Some(self.get_identifier()),
            );

            // wait for commit
            self.commit_notify.notified().await;

            let delay = self.transaction_interval;
            if !delay.is_zero() {
                asim::time::sleep(delay).await;
            }
        }
    }

    pub fn get_latencies(&self) -> Vec<Duration> {
        let latencies = self.latencies.borrow();
        latencies.clone()
    }

    pub fn get_account_id(&self) -> &AccountId {
        &self.account_id
    }

    pub(crate) fn notify_transaction_commit(&self) {
        let elapsed = {
            let issue_time = self
                .txn_issue_time
                .borrow()
                .expect("No transaction issue time");
            asim::time::now() - issue_time
        };

        log::trace!(
            "Committed transaction after {} seconds",
            elapsed.to_seconds()
        );

        {
            let mut latencies = self.latencies.borrow_mut();
            latencies.push(elapsed);
        }

        // wake up client loop
        self.commit_notify.notify_one();
    }
}

impl Object for Client {
    fn get_identifier(&self) -> ObjectId {
        self.identifier
    }
}
