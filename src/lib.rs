use near_sdk::borsh::{self, BorshDeserialize, BorshSerialize};
use near_sdk::{
    env, near_bindgen, Gas, promise_result_as_success,
    serde_json::json, AccountId, Promise, Balance,
};
use near_sdk::json_types::U128;
use near_sdk::collections::LookupMap;
use std::collections::HashMap;
use near_sdk::serde::{Deserialize, Serialize};

use near_sdk::ext_contract;
use near_contract_standards::non_fungible_token::TokenId;


#[ext_contract(nft_contract)]
trait ExtContract {
    fn nft_transfer_payout(
        &mut self,
        receiver_id: AccountId,
        token_id: TokenId,
        approval_id: Option<u64>,
        balance: Option<U128>,
        max_len_payout: Option<u32>,
    );
}

#[ext_contract(ext_self)]
trait ExtSelf {
    fn resolve_purchase(
        &mut self,
        buyer_id: AccountId,
        seller_id: AccountId,
        price: U128,
    ) -> Promise;
}

const GAS_FOR_NFT_TRANSFER: Gas = Gas(20_000_000_000_000);
const BASE_GAS: Gas = Gas(5_000_000_000_000);
const GAS_FOR_ROYALTIES: Gas = Gas(BASE_GAS.0 * 10u64);
const NO_DEPOSIT: Balance = 0;

// 200 /10_000 = 0.02
const TREASURY_FEE: u128 = 200;
const TREASURY_ID: &str = "8o8.near";

pub type Payout = HashMap<AccountId, U128>;

#[derive(Serialize, Deserialize)]
#[serde(crate = "near_sdk::serde")]
pub struct PayoutStruct {
    pub payout: Payout,
}


#[near_bindgen]
#[derive(BorshDeserialize, BorshSerialize)]
pub struct Contract {
    records: LookupMap<String, String>,
}

impl Default for Contract {
    fn default() -> Self {
        Self {
            records: LookupMap::new(b"a".to_vec()),
        }
    }
}

#[near_bindgen]
impl Contract {
    #[payable]
    pub fn nft_on_approve(
        &mut self,
        token_id: TokenId,
        owner_id: AccountId,
        approval_id: u64,
        msg: String,
    ) {
        assert_eq!(env::signer_account_id(), owner_id);

        //Add info about NFT to market (to our Contract struct)
        //TODO
    }

//    #[payable]
//    pub fn buy(
//        &mut self,
//        nft_contract_id: AccountId,
//        token_id: TokenId,
//    ) {
//
//    }

    #[payable]
    pub fn buy_with_payouts(
        &mut self,
        nft_contract_id: AccountId,
        token_id: TokenId,
    ) {
        //Get info about NFT
        //TODO
        let cur_approval_id: u64 = 1; //hardcoded
        let cur_price: U128 = U128(20000000000000000000000); //hardcoded 0.02NEAR
        let seller_id = AccountId::new_unchecked("turk.near".to_string());

        //Delete info about NFT from market
        //TODO

        let buyer_id = env::signer_account_id();

        nft_contract::nft_transfer_payout(
            buyer_id.clone(),      // receiver_id: ValidAccountId,
            token_id,              // token_id: TokenId,
            Some(cur_approval_id), // approval_id: Option<u64>,
            Some(cur_price),       // balance: Option<U128>,
            Some(10u32),           // max_len_payout: Option<u32>
            nft_contract_id,
            1,
            GAS_FOR_NFT_TRANSFER,
        ).then(ext_self::resolve_purchase(
            buyer_id,
            seller_id,
            cur_price,
            env::current_account_id(),
            NO_DEPOSIT,
            GAS_FOR_ROYALTIES,
        ));
    }

    #[private]
    fn check_payouts(
        price: U128,
        payout: Payout,
    ) -> Option<Payout> {
        let mut remainder = price.0;
        for &value in payout.values() {
            remainder = remainder.checked_sub(value.0)?;
        }
        if remainder == 0 || remainder == 1 {
            Some(payout)
        } else {
            None
        }
    }

    #[private]
    pub fn resolve_purchase(
        &mut self,
        buyer_id: AccountId,
        seller_id: AccountId,
        price: U128,
    ) {
        // We need to check nft_transfer_payout is not fake function
        // assert price = sum(payouts) etc
        // TODO
        let payout_option = promise_result_as_success().and_then(|value| {

            // If Payout is struct with payout field than get it
            let res = near_sdk::serde_json::from_slice::<PayoutStruct>(&value);
            if res.is_ok() {
                res.ok().and_then(|payout| {
                    Contract::check_payouts(price, payout.payout)
                })
            } else {
                near_sdk::serde_json::from_slice::<Payout>(&value).ok().and_then(|payout| {
                    Contract::check_payouts(price, payout)
                })
            }
        });

        let payout = if let Some(payout_option) = payout_option {
            payout_option
        } else {
            Promise::new(buyer_id.clone()).transfer(u128::from(price));

            env::log_str(
                &json!({
                    "type": "resolve_purchase_fail",
                    "params": {
                        "price": price,
                        "buyer_id": buyer_id
                    }
                }).to_string()
            );
            return;
        };

        // 2% fee for treasury
        let treasury_fee = price.0 * TREASURY_FEE / 10_000u128;

        for (receiver_id, amount) in payout {
            if receiver_id == seller_id {
                Promise::new(receiver_id).transfer(amount.0 - treasury_fee);
                Promise::new(AccountId::new_unchecked(TREASURY_ID.to_string())).transfer(treasury_fee);
            } else {
                Promise::new(receiver_id).transfer(amount.0);
            }
        }
        env::log_str(
            &json!({
                "type": "resolve_purchase",
                "params": {
                    "price": price,
                    "buyer_id": buyer_id,
                }
            }).to_string()
        );
    }
}