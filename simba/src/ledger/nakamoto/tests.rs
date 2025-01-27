use std::rc::Rc;

use crate::config::Difficulty;
use crate::logic::{Block, GENESIS_BLOCK, GENESIS_HEIGHT, Transaction, TransactionId};

use super::{NakamotoBlock, NakamotoNodeLedger};

use cow_tree::CowTree;

fn make_transaction() -> Rc<Transaction> {
    Rc::new(Transaction::new(rand::random(), 1))
}

fn make_initial_block(transactions: Vec<TransactionId>) -> Rc<NakamotoBlock> {
    let identifier = rand::random();
    let mined_by = rand::random();
    let uncles = vec![];

    Rc::new(NakamotoBlock::new_with_id(
        identifier,
        mined_by,
        GENESIS_BLOCK,
        uncles,
        GENESIS_HEIGHT + 1,
        0,
        Difficulty::default(),
        transactions,
        CowTree::default().freeze(),
    ))
}

fn make_next_block(
    prev: &Rc<NakamotoBlock>,
    transactions: Vec<TransactionId>,
) -> Rc<NakamotoBlock> {
    let identifier = rand::random();
    let uncles = vec![];

    Rc::new(NakamotoBlock::new_with_id(
        identifier,
        prev.get_miner(),
        prev.identifier,
        uncles,
        prev.get_height() + 1,
        0,
        Difficulty::default(),
        transactions,
        CowTree::default().freeze(),
    ))
}

#[asim::test]
async fn switch_chain_head() {
    let commit_delay = 10;

    let mut ledger = NakamotoNodeLedger::new();

    let mut fork1 = vec![];
    let mut fork2 = vec![];

    let start = make_initial_block(vec![]);
    ledger.add_new_block(start.clone(), commit_delay);

    let mut prev = start.clone();
    for _ in 0..15 {
        let tx = make_transaction();
        fork1.push(*tx.get_identifier());
        let block = make_next_block(&prev, vec![*tx.get_identifier()]);
        ledger.add_transaction(tx);
        ledger.add_new_block(block.clone(), commit_delay);
        prev = block;
    }

    for tx_id in fork1.iter() {
        assert!(ledger.is_transaction_applied(tx_id));
    }

    assert_eq!(ledger.forks.len(), 1);
    assert_eq!(&ledger.get_longest_chain().0, prev.get_identifier());

    let mut prev = start;
    for _ in 0..20 {
        let tx = make_transaction();
        fork2.push(*tx.get_identifier());
        let block = make_next_block(&prev, vec![*tx.get_identifier()]);
        ledger.add_transaction(tx);
        ledger.add_new_block(block.clone(), commit_delay);
        prev = block;
    }

    assert_eq!(ledger.forks.len(), 2);
    assert_eq!(&ledger.get_longest_chain().0, prev.get_identifier());

    for tx_id in fork1.iter() {
        assert!(!ledger.is_transaction_applied(tx_id));
    }

    for tx_id in fork2.iter() {
        assert!(ledger.is_transaction_applied(tx_id));
    }

    for tx_id in fork1.iter() {
        assert!(ledger.knows_transaction(tx_id));
    }

    for tx_id in fork2.iter() {
        assert!(ledger.knows_transaction(tx_id));
    }
}
