use cosmwasm_std::Uint128;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// Denomination of the token to be vested
pub const VEST_DENOM: &str = "umars";

#[derive(Serialize, Deserialize, Copy, Clone, Debug, PartialEq, JsonSchema)]
pub struct Schedule {
    /// Time when vesting/unlocking starts
    pub start_time: u64,
    /// Time before with no token is to be vested/unlocked
    pub cliff: u64,
    /// Duration of the vesting/unlocking process. At time `start_time + duration`, the tokens are
    /// vested/unlocked in full
    pub duration: u64,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct InstantiateMsg {
    /// The contract's owner
    pub owner: String,
    /// Schedule for token unlocking; this schedule is the same for all users
    pub unlock_schedule: Schedule,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ExecuteMsg {
    /// Create a new vesting position for a user
    CreatePosition {
        user: String,
        vest_schedule: Schedule,
    },
    /// Propose to transfer the contract's ownership to another account
    TransferOwnership {
        new_owner: String,
    },
    /// Accept the proposed ownership transfer
    AcceptOwnership {},
    /// Withdraw vested and unlocked MARS tokens
    Withdraw {},
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum QueryMsg {
    /// The contract's configurations; returns `ConfigResponse`
    Config {},
    /// Amount of MARS tokens currently locked in the vesting contract; returns `Uint128`
    TotalVotingPower {},
    /// Amount of MARS tokens of a vesting recipient current locked in the contract; returns `Uint128`
    VotingPower {
        user: String,
    },
    /// Details of a recipient's vesting position; returns `PositionResponse`
    ///
    /// NOTE: This query depends on block time, therefore it may not work with time travel queries
    Position {
        user: String,
    },
    /// Enumerate all vesting positions; returns `Vec<PositionResponse>`
    ///
    /// NOTE: This query depends on block time, therefore it may not work with time travel queries
    Positions {
        start_after: Option<String>,
        limit: Option<u32>,
    }
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct ConfigResponse {
    /// The contract's owner
    pub owner: String,
    /// If there is an ongoing transfer of ownership, address of the pending owner
    pub pending_owner: Option<String>,
    /// Schedule for token unlocking; this schedule is the same for all users
    pub unlock_schedule: Schedule,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct PositionResponse {
    /// Address of the user
    pub user: String,
    /// Total amount of MARS tokens allocated to this recipient
    pub total: Uint128,
    /// Amount of tokens that have been vested, according to the vesting schedule
    pub vested: Uint128,
    /// Amount of tokens that have been unlocked, according to the unlocking schedule
    pub unlocked: Uint128,
    /// Amount of tokens that have already been withdrawn
    pub withdrawn: Uint128,
    /// Amount of tokens that can be withdrawn now, defined as the smaller of vested and unlocked amounts,
    /// minus the amount already withdrawn
    pub withdrawable: Uint128,
    /// This vesting position's vesting schedule
    pub vest_schedule: Schedule,
}
