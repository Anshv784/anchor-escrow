use anchor_escrow::state::Escrow;
use anchor_lang::{
    prelude::{AccountMeta, Pubkey},
    solana_program::{instruction::Instruction, system_program},
    AccountDeserialize, InstructionData, ToAccountMetas,
};
use anchor_spl::associated_token::{self, get_associated_token_address};
use litesvm::LiteSVM;
use litesvm_token::{
    get_spl_account, spl_token::state::Account as TokenAccount, CreateAssociatedTokenAccount,
    CreateMint, MintTo,
};
use solana_keypair::Keypair;
use solana_message::{Message, VersionedMessage};
use solana_signer::Signer;
use solana_transaction::versioned::VersionedTransaction;

const SOL: u64 = 1_000_000_000;
const TOKEN_PROGRAM: Pubkey = anchor_spl::token::ID;

struct Ctx {
    svm: LiteSVM,
    maker: Keypair,
    taker: Keypair,
    mint_a: Pubkey,
    mint_b: Pubkey,
}

// maker starts with 1000 A, taker starts with 1000 B
fn setup() -> Ctx {
    let mut svm = LiteSVM::new();
    let bytes = include_bytes!(concat!(
        env!("CARGO_TARGET_TMPDIR"),
        "/../deploy/anchor_escrow.so"
    ));
    svm.add_program(anchor_escrow::id(), bytes).unwrap();

    let maker = Keypair::new();
    let taker = Keypair::new();
    svm.airdrop(&maker.pubkey(), 10 * SOL).unwrap();
    svm.airdrop(&taker.pubkey(), 10 * SOL).unwrap();

    let mint_a = CreateMint::new(&mut svm, &maker).decimals(6).send().unwrap();
    let mint_b = CreateMint::new(&mut svm, &taker).decimals(6).send().unwrap();

    let maker_ata_a = CreateAssociatedTokenAccount::new(&mut svm, &maker, &mint_a)
        .send()
        .unwrap();
    MintTo::new(&mut svm, &maker, &mint_a, &maker_ata_a, 1000).send().unwrap();

    let taker_ata_b = CreateAssociatedTokenAccount::new(&mut svm, &taker, &mint_b)
        .send()
        .unwrap();
    MintTo::new(&mut svm, &taker, &mint_b, &taker_ata_b, 1000).send().unwrap();

    Ctx { svm, maker, taker, mint_a, mint_b }
}

fn send(svm: &mut LiteSVM, ix: Instruction, signer: &Keypair) -> Result<(), String> {
    let msg = Message::new_with_blockhash(&[ix], Some(&signer.pubkey()), &svm.latest_blockhash());
    let tx = VersionedTransaction::try_new(VersionedMessage::Legacy(msg), &[signer]).unwrap();
    let res = svm.send_transaction(tx).map(|_| ()).map_err(|e| format!("{:?}", e.err));
    svm.expire_blockhash();
    res
}

fn escrow_pda(maker: &Pubkey, seed: u64) -> Pubkey {
    Pubkey::find_program_address(
        &[b"escrow", maker.as_ref(), &seed.to_le_bytes()],
        &anchor_escrow::id(),
    )
    .0
}

fn ata(owner: &Pubkey, mint: &Pubkey) -> Pubkey {
    get_associated_token_address(owner, mint)
}

fn token_balance(svm: &LiteSVM, ata: &Pubkey) -> u64 {
    get_spl_account::<TokenAccount>(svm, ata).unwrap().amount
}

fn exists(svm: &LiteSVM, key: &Pubkey) -> bool {
    svm.get_account(key).map(|a| a.lamports > 0).unwrap_or(false)
}

fn read_escrow(svm: &LiteSVM, key: &Pubkey) -> Escrow {
    let acc = svm.get_account(key).unwrap();
    Escrow::try_deserialize(&mut acc.data.as_slice()).unwrap()
}

impl Ctx {
    fn make_ix(&self, seed: u64, deposit: u64, receive: u64) -> Instruction {
        let maker = self.maker.pubkey();
        let escrow = escrow_pda(&maker, seed);
        Instruction::new_with_bytes(
            anchor_escrow::id(),
            &anchor_escrow::instruction::Make { seed, deposit, receive }.data(),
            anchor_escrow::accounts::Make {
                maker,
                mint_a: self.mint_a,
                mint_b: self.mint_b,
                maker_ata_a: ata(&maker, &self.mint_a),
                escrow,
                vault: ata(&escrow, &self.mint_a),
                associated_token_program: associated_token::ID,
                token_program: TOKEN_PROGRAM,
                system_program: system_program::ID,
            }
            .to_account_metas(None),
        )
    }

    fn take_ix(&self, taker: &Pubkey, seed: u64) -> Instruction {
        let maker = self.maker.pubkey();
        let escrow = escrow_pda(&maker, seed);
        Instruction::new_with_bytes(
            anchor_escrow::id(),
            &anchor_escrow::instruction::Take {}.data(),
            anchor_escrow::accounts::Take {
                taker: *taker,
                maker,
                mint_a: self.mint_a,
                mint_b: self.mint_b,
                taker_ata_a: ata(taker, &self.mint_a),
                taker_ata_b: ata(taker, &self.mint_b),
                maker_ata_b: ata(&maker, &self.mint_b),
                escrow,
                vault: ata(&escrow, &self.mint_a),
                associated_token_program: associated_token::ID,
                token_program: TOKEN_PROGRAM,
                system_program: system_program::ID,
            }
            .to_account_metas(None),
        )
    }

    fn refund_accounts(&self, maker: &Pubkey, seed: u64) -> Vec<AccountMeta> {
        let escrow = escrow_pda(&self.maker.pubkey(), seed);
        anchor_escrow::accounts::Refund {
            maker: *maker,
            mint_a: self.mint_a,
            maker_ata_a: ata(maker, &self.mint_a),
            escrow,
            vault: ata(&escrow, &self.mint_a),
            associated_token_program: associated_token::ID,
            token_program: TOKEN_PROGRAM,
            system_program: system_program::ID,
        }
        .to_account_metas(None)
    }

    fn refund_ix(&self, seed: u64) -> Instruction {
        Instruction::new_with_bytes(
            anchor_escrow::id(),
            &anchor_escrow::instruction::Refund {}.data(),
            self.refund_accounts(&self.maker.pubkey(), seed),
        )
    }

    fn update_ix(&self, signer: &Pubkey, seed: u64, receive: u64) -> Instruction {
        Instruction::new_with_bytes(
            anchor_escrow::id(),
            &anchor_escrow::instruction::Update { receive }.data(),
            anchor_escrow::accounts::Update {
                maker: *signer,
                escrow: escrow_pda(&self.maker.pubkey(), seed),
            }
            .to_account_metas(None),
        )
    }
}

#[test]
fn make_locks_tokens_in_vault() {
    let mut c = setup();
    let ix = c.make_ix(1, 400, 250);
    send(&mut c.svm, ix, &c.maker).unwrap();

    let escrow = escrow_pda(&c.maker.pubkey(), 1);
    let state = read_escrow(&c.svm, &escrow);
    assert_eq!(state.seed, 1);
    assert_eq!(state.maker, c.maker.pubkey());
    assert_eq!(state.mint_a, c.mint_a);
    assert_eq!(state.mint_b, c.mint_b);
    assert_eq!(state.receive, 250);

    let vault = get_spl_account::<TokenAccount>(&c.svm, &ata(&escrow, &c.mint_a)).unwrap();
    assert_eq!(vault.amount, 400);
    assert_eq!(vault.owner, escrow);
    assert_eq!(token_balance(&c.svm, &ata(&c.maker.pubkey(), &c.mint_a)), 600);
}

#[test]
fn maker_can_have_two_escrows() {
    let mut c = setup();
    let ix = c.make_ix(1, 100, 10);
    send(&mut c.svm, ix, &c.maker).unwrap();
    let ix = c.make_ix(2, 200, 20);
    send(&mut c.svm, ix, &c.maker).unwrap();

    assert_eq!(read_escrow(&c.svm, &escrow_pda(&c.maker.pubkey(), 1)).receive, 10);
    assert_eq!(read_escrow(&c.svm, &escrow_pda(&c.maker.pubkey(), 2)).receive, 20);
    assert_eq!(token_balance(&c.svm, &ata(&c.maker.pubkey(), &c.mint_a)), 700);
}

#[test]
fn make_with_zero_fails() {
    let mut c = setup();
    let ix = c.make_ix(1, 0, 250);
    assert!(send(&mut c.svm, ix, &c.maker).unwrap_err().contains("Custom(6000)"));
    let ix = c.make_ix(1, 400, 0);
    assert!(send(&mut c.svm, ix, &c.maker).unwrap_err().contains("Custom(6000)"));
}

#[test]
fn make_more_than_balance_fails() {
    let mut c = setup();
    let ix = c.make_ix(1, 1001, 250);
    assert!(send(&mut c.svm, ix, &c.maker).is_err());
}

#[test]
fn take_swaps_and_closes() {
    let mut c = setup();
    let ix = c.make_ix(7, 400, 250);
    send(&mut c.svm, ix, &c.maker).unwrap();

    let escrow = escrow_pda(&c.maker.pubkey(), 7);
    let vault = ata(&escrow, &c.mint_a);
    let maker_sol = c.svm.get_balance(&c.maker.pubkey()).unwrap();
    let rent_back = c.svm.get_balance(&escrow).unwrap() + c.svm.get_balance(&vault).unwrap();

    let ix = c.take_ix(&c.taker.pubkey(), 7);
    send(&mut c.svm, ix, &c.taker).unwrap();

    let maker = c.maker.pubkey();
    let taker = c.taker.pubkey();
    assert_eq!(token_balance(&c.svm, &ata(&taker, &c.mint_a)), 400);
    assert_eq!(token_balance(&c.svm, &ata(&taker, &c.mint_b)), 750);
    assert_eq!(token_balance(&c.svm, &ata(&maker, &c.mint_b)), 250);
    assert_eq!(token_balance(&c.svm, &ata(&maker, &c.mint_a)), 600);

    assert!(!exists(&c.svm, &escrow));
    assert!(!exists(&c.svm, &vault));
    // rent for both closed accounts goes to the maker, not the taker
    assert_eq!(c.svm.get_balance(&maker).unwrap(), maker_sol + rent_back);
}

#[test]
fn take_twice_fails() {
    let mut c = setup();
    let ix = c.make_ix(1, 400, 250);
    send(&mut c.svm, ix, &c.maker).unwrap();
    let ix = c.take_ix(&c.taker.pubkey(), 1);
    send(&mut c.svm, ix, &c.taker).unwrap();
    let ix = c.take_ix(&c.taker.pubkey(), 1);
    assert!(send(&mut c.svm, ix, &c.taker).is_err());
}

#[test]
fn take_without_enough_b_fails() {
    let mut c = setup();
    let ix = c.make_ix(1, 400, 1001);
    send(&mut c.svm, ix, &c.maker).unwrap();

    let ix = c.take_ix(&c.taker.pubkey(), 1);
    assert!(send(&mut c.svm, ix, &c.taker).is_err());

    // nothing moved
    let escrow = escrow_pda(&c.maker.pubkey(), 1);
    assert_eq!(token_balance(&c.svm, &ata(&escrow, &c.mint_a)), 400);
    assert_eq!(token_balance(&c.svm, &ata(&c.taker.pubkey(), &c.mint_b)), 1000);
}

#[test]
fn take_with_wrong_mint_fails() {
    let mut c = setup();
    let ix = c.make_ix(1, 400, 250);
    send(&mut c.svm, ix, &c.maker).unwrap();

    // taker tries to pay with some other token they made themselves
    let fake = CreateMint::new(&mut c.svm, &c.taker).decimals(6).send().unwrap();
    let fake_ata = CreateAssociatedTokenAccount::new(&mut c.svm, &c.taker, &fake)
        .send()
        .unwrap();
    MintTo::new(&mut c.svm, &c.taker, &fake, &fake_ata, 1000).send().unwrap();

    let real_b = c.mint_b;
    c.mint_b = fake;
    let ix = c.take_ix(&c.taker.pubkey(), 1);
    c.mint_b = real_b;
    let err = send(&mut c.svm, ix, &c.taker).unwrap_err();
    // 2001 = ConstraintHasOne
    assert!(err.contains("Custom(2001)"), "{err}");
}

#[test]
fn refund_returns_tokens_and_closes() {
    let mut c = setup();
    let ix = c.make_ix(3, 400, 250);
    send(&mut c.svm, ix, &c.maker).unwrap();

    let ix = c.refund_ix(3);
    send(&mut c.svm, ix, &c.maker).unwrap();

    let escrow = escrow_pda(&c.maker.pubkey(), 3);
    assert_eq!(token_balance(&c.svm, &ata(&c.maker.pubkey(), &c.mint_a)), 1000);
    assert!(!exists(&c.svm, &escrow));
    assert!(!exists(&c.svm, &ata(&escrow, &c.mint_a)));
}

#[test]
fn only_maker_can_refund() {
    let mut c = setup();
    let ix = c.make_ix(1, 400, 250);
    send(&mut c.svm, ix, &c.maker).unwrap();

    let ix = Instruction::new_with_bytes(
        anchor_escrow::id(),
        &anchor_escrow::instruction::Refund {}.data(),
        c.refund_accounts(&c.taker.pubkey(), 1),
    );
    assert!(send(&mut c.svm, ix, &c.taker).is_err());
    let escrow = escrow_pda(&c.maker.pubkey(), 1);
    assert_eq!(token_balance(&c.svm, &ata(&escrow, &c.mint_a)), 400);
}

#[test]
fn update_changes_price() {
    let mut c = setup();
    let ix = c.make_ix(1, 400, 250);
    send(&mut c.svm, ix, &c.maker).unwrap();

    let ix = c.update_ix(&c.maker.pubkey(), 1, 300);
    send(&mut c.svm, ix, &c.maker).unwrap();
    assert_eq!(read_escrow(&c.svm, &escrow_pda(&c.maker.pubkey(), 1)).receive, 300);

    // taker now pays the new price
    let ix = c.take_ix(&c.taker.pubkey(), 1);
    send(&mut c.svm, ix, &c.taker).unwrap();
    assert_eq!(token_balance(&c.svm, &ata(&c.maker.pubkey(), &c.mint_b)), 300);
}

#[test]
fn only_maker_can_update() {
    let mut c = setup();
    let ix = c.make_ix(1, 400, 250);
    send(&mut c.svm, ix, &c.maker).unwrap();

    let ix = c.update_ix(&c.taker.pubkey(), 1, 1);
    assert!(send(&mut c.svm, ix, &c.taker).is_err());
    assert_eq!(read_escrow(&c.svm, &escrow_pda(&c.maker.pubkey(), 1)).receive, 250);
}

#[test]
fn update_to_zero_fails() {
    let mut c = setup();
    let ix = c.make_ix(1, 400, 250);
    send(&mut c.svm, ix, &c.maker).unwrap();
    let ix = c.update_ix(&c.maker.pubkey(), 1, 0);
    assert!(send(&mut c.svm, ix, &c.maker).unwrap_err().contains("Custom(6000)"));
}
