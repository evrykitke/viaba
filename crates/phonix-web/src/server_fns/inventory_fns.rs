//! Inventory: what the workspace stocks, where it is, and how it got there.
//!
//! There is deliberately no endpoint here that reads the chart of accounts.
//! The account picker on an item goes through `phonix_ports::Ledger`, so the
//! browser never crosses the app boundary either - see ADR 0006 section 2 and
//! [`postable_accounts`] below, which is the port's own answer rather than a
//! query against `books`.

use app_inventory::accounts::{AccountOverrides, AccountRef};
use app_inventory::bill::{Bill, BillInput, BillSummary, MatchGrade, UnbilledReceipt};
use app_inventory::transfer::{
    ArrivalInput, Transfer, TransferInput, TransferSummary,
};
use app_inventory::landed_cost::{
    Landable, LandedCost, LandedCostInput, LandedCostSummary, ReceiptLandedCost,
};
use app_inventory::category::{Category, CategoryInput, CategorySummary};
use app_inventory::consolidation::{
    Consolidation, ConsolidationInput, ConsolidationSummary, LineAllocation,
};
use app_inventory::image::{Gallery, ImageInput};
use app_inventory::item::{Item, ItemInput, ItemSummary};
use app_inventory::location::{Location, LocationInput, LocationSummary};
use app_inventory::lot::LotSummary;
use app_inventory::movement::{MoveFilter, MoveSummary, StockMove};
use app_inventory::purchase::{OrderInput, OrderSummary, PurchaseOrder};
use app_inventory::quant::{OnHandFilter, OnHandRow};
use app_inventory::receipt::{Backorder, Receipt, ReceiptInput, ReceiptSummary};
use app_inventory::requisition::{
    DecisionInput, Demand, Requisition, RequisitionInput, RequisitionSummary,
};
use app_inventory::quantity::Quantity;
use app_inventory::unit::{Unit, UnitInput};
use app_inventory::variant::{Attribute, Plan, Selection, VariantChoice, VariantSummary};
use app_inventory::warehouse::{Warehouse, WarehouseInput, WarehouseSummary};
use leptos::prelude::*;
use leptos::server_fn::codec::Json;
use phonix_core::form::Submission;
use uuid::Uuid;

// --- Units ---------------------------------------------------------------

#[server(name = ListUnits, prefix = "/api", endpoint = "inventory/units")]
pub async fn list_units() -> Result<Vec<Unit>, ServerFnError> {
    use crate::state::{pool_and_caller, service_error};

    let (pool, caller) = pool_and_caller().await?;

    phonix_services::inventory::unit::list(&pool, &caller)
        .await
        .map_err(service_error)
}

/// The units a picker offers. Gated on items rather than units: choosing what
/// an item is counted in is not the same power as redrawing the unit list.
#[server(name = SelectableUnits, prefix = "/api", endpoint = "inventory/units/selectable")]
pub async fn selectable_units() -> Result<Vec<Unit>, ServerFnError> {
    use crate::state::{pool_and_caller, service_error};

    let (pool, caller) = pool_and_caller().await?;

    phonix_services::inventory::unit::selectable(&pool, &caller)
        .await
        .map_err(service_error)
}

#[server(name = UnitEdit, prefix = "/api", endpoint = "inventory/units/edit")]
pub async fn unit_edit(unit_id: Uuid) -> Result<UnitInput, ServerFnError> {
    use crate::state::{pool_and_caller, service_error};

    let (pool, caller) = pool_and_caller().await?;

    phonix_services::inventory::unit::edit(&pool, &caller, unit_id)
        .await
        .map_err(service_error)
}

#[server(name = SaveUnit, prefix = "/api", endpoint = "inventory/units/save")]
pub async fn save_unit(draft: UnitInput) -> Result<Submission<UnitInput>, ServerFnError> {
    use crate::state::{pool_and_caller, service_error};

    let (pool, caller) = pool_and_caller().await?;

    phonix_services::inventory::unit::save(&pool, &caller, draft)
        .await
        .map_err(service_error)
}

#[server(name = DeleteUnit, prefix = "/api", endpoint = "inventory/units/delete")]
pub async fn delete_unit(
    unit_id: Uuid,
) -> Result<app_inventory::unit::DeleteOutcome, ServerFnError> {
    use crate::state::{pool_and_caller, service_error};

    let (pool, caller) = pool_and_caller().await?;

    phonix_services::inventory::unit::delete(&pool, &caller, unit_id)
        .await
        .map_err(service_error)
}

// --- Locations -----------------------------------------------------------

#[server(name = ListStockLocations, prefix = "/api", endpoint = "inventory/locations")]
pub async fn list_stock_locations() -> Result<Vec<LocationSummary>, ServerFnError> {
    use crate::state::{pool_and_caller, service_error};

    let (pool, caller) = pool_and_caller().await?;

    phonix_services::inventory::location::list(&pool, &caller)
        .await
        .map_err(service_error)
}

#[server(name = SelectableLocations, prefix = "/api", endpoint = "inventory/locations/selectable")]
pub async fn selectable_locations() -> Result<Vec<Location>, ServerFnError> {
    use crate::state::{pool_and_caller, service_error};

    let (pool, caller) = pool_and_caller().await?;

    phonix_services::inventory::location::selectable(&pool, &caller)
        .await
        .map_err(service_error)
}

#[server(name = StockLocationEdit, prefix = "/api", endpoint = "inventory/locations/edit")]
pub async fn stock_location_edit(location_id: Uuid) -> Result<LocationInput, ServerFnError> {
    use crate::state::{pool_and_caller, service_error};

    let (pool, caller) = pool_and_caller().await?;

    phonix_services::inventory::location::edit(&pool, &caller, location_id)
        .await
        .map_err(service_error)
}

#[server(name = SaveStockLocation, prefix = "/api", endpoint = "inventory/locations/save")]
pub async fn save_stock_location(
    draft: LocationInput,
) -> Result<Submission<LocationInput>, ServerFnError> {
    use crate::state::{pool_and_caller, service_error};

    let (pool, caller) = pool_and_caller().await?;

    phonix_services::inventory::location::save(&pool, &caller, draft)
        .await
        .map_err(service_error)
}

#[server(name = DeleteStockLocation, prefix = "/api", endpoint = "inventory/locations/delete")]
pub async fn delete_stock_location(
    location_id: Uuid,
) -> Result<app_inventory::location::DeleteOutcome, ServerFnError> {
    use crate::state::{pool_and_caller, service_error};

    let (pool, caller) = pool_and_caller().await?;

    phonix_services::inventory::location::delete(&pool, &caller, location_id)
        .await
        .map_err(service_error)
}

// --- Warehouses ----------------------------------------------------------

#[server(name = ListWarehouses, prefix = "/api", endpoint = "inventory/warehouses")]
pub async fn list_warehouses() -> Result<Vec<WarehouseSummary>, ServerFnError> {
    use crate::state::{pool_and_caller, service_error};

    let (pool, caller) = pool_and_caller().await?;

    phonix_services::inventory::warehouse::list(&pool, &caller)
        .await
        .map_err(service_error)
}

#[server(name = SelectableWarehouses, prefix = "/api", endpoint = "inventory/warehouses/selectable")]
pub async fn selectable_warehouses() -> Result<Vec<Warehouse>, ServerFnError> {
    use crate::state::{pool_and_caller, service_error};

    let (pool, caller) = pool_and_caller().await?;

    phonix_services::inventory::warehouse::selectable(&pool, &caller)
        .await
        .map_err(service_error)
}

#[server(name = WarehouseEdit, prefix = "/api", endpoint = "inventory/warehouses/edit")]
pub async fn warehouse_edit(warehouse_id: Uuid) -> Result<WarehouseInput, ServerFnError> {
    use crate::state::{pool_and_caller, service_error};

    let (pool, caller) = pool_and_caller().await?;

    phonix_services::inventory::warehouse::edit(&pool, &caller, warehouse_id)
        .await
        .map_err(service_error)
}

#[server(name = SaveWarehouse, prefix = "/api", endpoint = "inventory/warehouses/save")]
pub async fn save_warehouse(
    draft: WarehouseInput,
) -> Result<Submission<WarehouseInput>, ServerFnError> {
    use crate::state::{pool_and_caller, service_error};

    let (pool, caller) = pool_and_caller().await?;

    phonix_services::inventory::warehouse::save(&pool, &caller, draft)
        .await
        .map_err(service_error)
}

/// Switch a warehouse off, or back on.
///
/// There is no delete: a warehouse owns the locations that carry every movement
/// that ever crossed them, so the honest operation is the one offered.
#[server(name = SetWarehouseActive, prefix = "/api", endpoint = "inventory/warehouses/active")]
pub async fn set_warehouse_active(
    warehouse_id: Uuid,
    active: bool,
) -> Result<Submission<WarehouseInput>, ServerFnError> {
    use crate::state::{pool_and_caller, service_error};

    let (pool, caller) = pool_and_caller().await?;

    phonix_services::inventory::warehouse::set_active(&pool, &caller, warehouse_id, active)
        .await
        .map_err(service_error)
}

// --- Categories ----------------------------------------------------------

#[server(name = ListItemCategories, prefix = "/api", endpoint = "inventory/categories")]
pub async fn list_item_categories() -> Result<Vec<CategorySummary>, ServerFnError> {
    use crate::state::{pool_and_caller, service_error};

    let (pool, caller) = pool_and_caller().await?;

    phonix_services::inventory::category::list(&pool, &caller)
        .await
        .map_err(service_error)
}

#[server(name = SelectableCategories, prefix = "/api", endpoint = "inventory/categories/selectable")]
pub async fn selectable_categories() -> Result<Vec<Category>, ServerFnError> {
    use crate::state::{pool_and_caller, service_error};

    let (pool, caller) = pool_and_caller().await?;

    phonix_services::inventory::category::selectable(&pool, &caller)
        .await
        .map_err(service_error)
}

#[server(name = ItemCategoryEdit, prefix = "/api", endpoint = "inventory/categories/edit")]
pub async fn item_category_edit(category_id: Uuid) -> Result<CategoryInput, ServerFnError> {
    use crate::state::{pool_and_caller, service_error};

    let (pool, caller) = pool_and_caller().await?;

    phonix_services::inventory::category::edit(&pool, &caller, category_id)
        .await
        .map_err(service_error)
}

#[server(name = SaveItemCategory, prefix = "/api", endpoint = "inventory/categories/save")]
pub async fn save_item_category(
    draft: CategoryInput,
) -> Result<Submission<CategoryInput>, ServerFnError> {
    use crate::state::{pool_and_caller, service_error};

    let (pool, caller) = pool_and_caller().await?;

    phonix_services::inventory::category::save(&pool, &caller, draft)
        .await
        .map_err(service_error)
}

#[server(name = DeleteItemCategory, prefix = "/api", endpoint = "inventory/categories/delete")]
pub async fn delete_item_category(
    category_id: Uuid,
) -> Result<app_inventory::category::DeleteOutcome, ServerFnError> {
    use crate::state::{pool_and_caller, service_error};

    let (pool, caller) = pool_and_caller().await?;

    phonix_services::inventory::category::delete(&pool, &caller, category_id)
        .await
        .map_err(service_error)
}

// --- Items ---------------------------------------------------------------

#[server(name = ListItems, prefix = "/api", endpoint = "inventory/items")]
pub async fn list_items() -> Result<Vec<ItemSummary>, ServerFnError> {
    use crate::state::{pool_and_caller, service_error};

    let (pool, caller) = pool_and_caller().await?;

    phonix_services::inventory::item::list(&pool, &caller)
        .await
        .map_err(service_error)
}

#[server(name = ItemDetail, prefix = "/api", endpoint = "inventory/items/detail")]
pub async fn item_detail(item_id: Uuid) -> Result<Item, ServerFnError> {
    use crate::state::{pool_and_caller, service_error};

    let (pool, caller) = pool_and_caller().await?;

    phonix_services::inventory::item::detail(&pool, &caller, item_id)
        .await
        .map_err(service_error)
}

#[server(name = ItemEdit, prefix = "/api", endpoint = "inventory/items/edit")]
pub async fn item_edit(item_id: Uuid) -> Result<ItemInput, ServerFnError> {
    use crate::state::{pool_and_caller, service_error};

    let (pool, caller) = pool_and_caller().await?;

    phonix_services::inventory::item::edit(&pool, &caller, item_id)
        .await
        .map_err(service_error)
}

#[server(name = SaveItem, prefix = "/api", endpoint = "inventory/items/save")]
pub async fn save_item(draft: ItemInput) -> Result<Submission<ItemInput>, ServerFnError> {
    use crate::state::{pool_and_caller, service_error};

    let (pool, caller) = pool_and_caller().await?;

    phonix_services::inventory::item::save(&pool, &caller, draft)
        .await
        .map_err(service_error)
}

#[server(name = DeleteItem, prefix = "/api", endpoint = "inventory/items/delete")]
pub async fn delete_item(
    item_id: Uuid,
) -> Result<app_inventory::item::DeleteOutcome, ServerFnError> {
    use crate::state::{pool_and_caller, service_error};

    let (pool, caller) = pool_and_caller().await?;

    phonix_services::inventory::item::delete(&pool, &caller, item_id)
        .await
        .map_err(service_error)
}

/// What a scanner produces: the item, and the variant if the code was one of
/// those. One call, because the person holding the scanner does not know which
/// of the two the string is in.
#[server(name = ItemByBarcode, prefix = "/api", endpoint = "inventory/items/scan")]
pub async fn item_by_barcode(
    barcode: String,
) -> Result<Option<(Item, Option<Uuid>)>, ServerFnError> {
    use crate::state::{pool_and_caller, service_error};

    let (pool, caller) = pool_and_caller().await?;

    phonix_services::inventory::item::by_barcode(&pool, &caller, &barcode)
        .await
        .map_err(service_error)
}

// --- Variants ------------------------------------------------------------

#[server(name = ItemVariants, prefix = "/api", endpoint = "inventory/items/variants")]
pub async fn item_variants(item_id: Uuid) -> Result<Vec<VariantSummary>, ServerFnError> {
    use crate::state::{pool_and_caller, service_error};

    let (pool, caller) = pool_and_caller().await?;

    phonix_services::inventory::item::variants_of(&pool, &caller, item_id)
        .await
        .map_err(service_error)
}

#[server(name = ItemSelection, prefix = "/api", endpoint = "inventory/items/selection")]
pub async fn item_selection(item_id: Uuid) -> Result<Selection, ServerFnError> {
    use crate::state::{pool_and_caller, service_error};

    let (pool, caller) = pool_and_caller().await?;

    phonix_services::inventory::item::selection(&pool, &caller, item_id)
        .await
        .map_err(service_error)
}

/// What changing the selection would do, without doing it.
///
/// Six colours, five sizes and three materials is ninety variants, and a
/// workspace that meant to add one colour should see that number first.
#[server(name = PlanVariants, prefix = "/api", endpoint = "inventory/items/variants/plan")]
pub async fn plan_variants(item_id: Uuid, selection: Selection) -> Result<Plan, ServerFnError> {
    use crate::state::{pool_and_caller, service_error};

    let (pool, caller) = pool_and_caller().await?;

    phonix_services::inventory::item::plan_variants(&pool, &caller, item_id, selection)
        .await
        .map_err(service_error)
}

#[server(name = SetVariants, prefix = "/api", endpoint = "inventory/items/variants/apply")]
pub async fn set_variants(item_id: Uuid, selection: Selection) -> Result<Plan, ServerFnError> {
    use crate::state::{pool_and_caller, service_error};

    let (pool, caller) = pool_and_caller().await?;

    phonix_services::inventory::item::set_variants(&pool, &caller, item_id, selection)
        .await
        .map_err(service_error)
}

#[server(name = SaveVariant, prefix = "/api", endpoint = "inventory/items/variants/save")]
pub async fn save_variant(
    variant_id: Uuid,
    barcode: Option<String>,
    price_extra: String,
    cost_extra: String,
) -> Result<bool, ServerFnError> {
    use crate::state::{pool_and_caller, service_error};

    let (pool, caller) = pool_and_caller().await?;

    phonix_services::inventory::item::save_variant(
        &pool,
        &caller,
        variant_id,
        barcode,
        price_extra,
        cost_extra,
    )
    .await
    .map_err(service_error)
}

#[server(name = ListAttributes, prefix = "/api", endpoint = "inventory/attributes")]
pub async fn list_attributes() -> Result<Vec<Attribute>, ServerFnError> {
    use crate::state::{pool_and_caller, service_error};

    let (pool, caller) = pool_and_caller().await?;

    phonix_services::inventory::item::attributes(&pool, &caller)
        .await
        .map_err(service_error)
}

// --- Pictures ------------------------------------------------------------

#[server(name = ItemGallery, prefix = "/api", endpoint = "inventory/items/images")]
pub async fn item_gallery(item_id: Uuid) -> Result<Gallery, ServerFnError> {
    use crate::state::{pool_and_caller, service_error};

    let (pool, caller) = pool_and_caller().await?;

    phonix_services::inventory::item::gallery(&pool, &caller, item_id)
        .await
        .map_err(service_error)
}

/// File an already-uploaded picture against an item, or one of its variants.
///
/// The upload itself went through the files endpoint, which is what checked the
/// bucket, the size and that it really is an image.
#[server(name = AttachItemImage, prefix = "/api", endpoint = "inventory/items/images/attach")]
pub async fn attach_item_image(draft: ImageInput) -> Result<Uuid, ServerFnError> {
    use crate::state::{pool_and_caller, service_error};

    let (pool, caller) = pool_and_caller().await?;

    phonix_services::inventory::item::attach_image(&pool, &caller, draft)
        .await
        .map_err(service_error)
}

#[server(name = DetachItemImage, prefix = "/api", endpoint = "inventory/items/images/detach")]
pub async fn detach_item_image(image_id: Uuid) -> Result<bool, ServerFnError> {
    use crate::state::{pool_and_caller, service_error};

    let (pool, caller) = pool_and_caller().await?;

    phonix_services::inventory::item::detach_image(&pool, &caller, image_id)
        .await
        .map_err(service_error)
}

// --- Account mapping -----------------------------------------------------

/// What the item overrides, and what its category does, so a screen can say
/// which of the two an account came from.
#[server(name = ItemAccounts, prefix = "/api", endpoint = "inventory/items/accounts")]
pub async fn item_accounts(
    item_id: Uuid,
) -> Result<(AccountOverrides, AccountOverrides), ServerFnError> {
    use crate::state::{pool_and_caller, service_error};

    let (pool, caller) = pool_and_caller().await?;

    phonix_services::inventory::item::account_overrides(&pool, &caller, item_id)
        .await
        .map_err(service_error)
}

/// The accounts an override may name.
///
/// Through the `Ledger` port, not a query against `books`. That is the whole
/// point: the browser gets a list of ids and labels, and Inventory still does
/// not depend on the accounting app. Empty where there is no ledger, so the
/// picker renders as "the default for this role" and the screen still works.
#[server(name = PostableAccounts, prefix = "/api", endpoint = "inventory/accounts")]
pub async fn postable_accounts()
-> Result<Vec<phonix_ports::ledger::LedgerAccount>, ServerFnError> {
    use phonix_ports::Ledger;

    use crate::state::{pool_and_caller, service_error};

    let (pool, caller) = pool_and_caller().await?;

    phonix_services::inventory::item::list(&pool, &caller)
        .await
        .map_err(service_error)?;

    phonix_services::books::BooksLedger::new(pool, caller)
        .postable_accounts()
        .await
        .map_err(|err| ServerFnError::new(err.to_string()))
}

/// Point one of an item's or a category's roles at an account, or stop
/// overriding it. `chosen` absent clears the override.
#[server(name = SetItemAccount, prefix = "/api", endpoint = "inventory/items/accounts/set")]
pub async fn set_item_account(
    owner_kind: String,
    owner_id: Uuid,
    role: String,
    chosen: Option<AccountRef>,
) -> Result<(), ServerFnError> {
    use phonix_db::inventory::account_mapping::Owner;
    use phonix_ports::ledger::AccountRole;

    use crate::state::{pool_and_caller, service_error};

    let (pool, caller) = pool_and_caller().await?;

    let owner = match owner_kind.as_str() {
        "category" => Owner::Category,
        "item" => Owner::Item,
        _ => return Err(ServerFnError::new("unknown owner")),
    };

    let role = AccountRole::parse(&role).ok_or_else(|| ServerFnError::new("unknown role"))?;

    phonix_services::inventory::item::set_account(&pool, &caller, owner, owner_id, role, chosen)
        .await
        .map_err(service_error)
}


// --- Stock ---------------------------------------------------------------
//
// Every one of these goes through the `Ledger` port for its accounting side.
// The browser never names an account and this module never queries `books` -
// see the header, and ADR 0006 section 2.

/// What is on hand, wherever it is.
#[server(name = StockOnHand, prefix = "/api", endpoint = "inventory/stock")]
pub async fn stock_on_hand(filter: OnHandFilter) -> Result<Vec<OnHandRow>, ServerFnError> {
    use crate::state::{pool_and_caller, service_error};

    let (pool, caller) = pool_and_caller().await?;

    phonix_services::inventory::stock::on_hand(&pool, &caller, filter)
        .await
        .map_err(service_error)
}

/// The movement history: every change to every quantity, newest first.
///
/// `Json` because every field of [`MoveFilter`] is optional - see the note on
/// `list_invoices` for what an unfiltered first load posts otherwise.
#[server(name = StockMoves, prefix = "/api", endpoint = "inventory/stock/moves", input = Json)]
pub async fn stock_moves(filter: MoveFilter) -> Result<Vec<MoveSummary>, ServerFnError> {
    use crate::state::{pool_and_caller, service_error};

    let (pool, caller) = pool_and_caller().await?;

    phonix_services::inventory::stock::moves(&pool, &caller, filter)
        .await
        .map_err(service_error)
}

/// What the workspace holds in stock, at cost. The figure a stock account is
/// reconciled against.
#[server(name = StockValue, prefix = "/api", endpoint = "inventory/stock/value")]
pub async fn stock_value() -> Result<String, ServerFnError> {
    use crate::state::{pool_and_caller, service_error};

    let (pool, caller) = pool_and_caller().await?;

    phonix_services::inventory::stock::total_value(&pool, &caller)
        .await
        .map(|value| value.to_storage_string())
        .map_err(service_error)
}

/// The lots of one variant, in the order a pick reaches for them.
#[server(name = VariantLots, prefix = "/api", endpoint = "inventory/stock/lots")]
pub async fn variant_lots(variant_id: Uuid) -> Result<Vec<LotSummary>, ServerFnError> {
    use crate::state::{pool_and_caller, service_error};

    let (pool, caller) = pool_and_caller().await?;

    phonix_services::inventory::stock::lots_of(&pool, &caller, variant_id)
        .await
        .map_err(service_error)
}

/// Write stock off, scrap it, or book in a count difference.
///
/// `found` says which way: true books stock in from inventory loss, false
/// writes it out to it. Both post through the `Ledger` port, and both refuse
/// outright if the ledger refuses - a shelf that changed while the stock
/// account did not is the thing this whole design exists to prevent.
#[server(name = AdjustStock, prefix = "/api", endpoint = "inventory/stock/adjust")]
pub async fn adjust_stock(
    location_id: Uuid,
    variant_id: Uuid,
    lot_id: Option<Uuid>,
    quantity: Quantity,
    found: bool,
    moved_on: chrono::NaiveDate,
    reason: Option<String>,
) -> Result<Submission<StockMove>, ServerFnError> {
    use crate::state::{pool_and_caller, service_error};

    let (pool, caller) = pool_and_caller().await?;
    let ledger = phonix_services::books::BooksLedger::new(pool.clone(), caller.clone());

    phonix_services::inventory::stock::adjust(
        &pool,
        &caller,
        &ledger,
        location_id,
        variant_id,
        lot_id,
        quantity,
        found,
        moved_on,
        reason,
    )
    .await
    .map_err(service_error)
}


// --- Requisitions --------------------------------------------------------
//
// The document before the order, and the one that commits nothing: no supplier,
// no price, no currency, and nothing that reaches the ledger. What it does carry
// is the cost centre, which is this app's first caller of the `CostCentres` port
// from a document - see ADR 0006 sections 2 and 7.

#[server(name = ListRequisitions, prefix = "/api", endpoint = "inventory/requisitions")]
pub async fn list_requisitions() -> Result<Vec<RequisitionSummary>, ServerFnError> {
    use crate::state::{pool_and_caller, service_error};

    let (pool, caller) = pool_and_caller().await?;

    phonix_services::inventory::requisition::list(&pool, &caller)
        .await
        .map_err(service_error)
}

/// What is waiting on a decision, for whoever answers them.
#[server(name = RequisitionsAwaiting, prefix = "/api", endpoint = "inventory/requisitions/awaiting")]
pub async fn requisitions_awaiting_decision() -> Result<Vec<RequisitionSummary>, ServerFnError> {
    use crate::state::{pool_and_caller, service_error};

    let (pool, caller) = pool_and_caller().await?;

    phonix_services::inventory::requisition::awaiting_decision(&pool, &caller)
        .await
        .map_err(service_error)
}

/// The cost centres a requisition may be charged to.
///
/// The port's answer, not a query against `hr` - the browser does not cross the
/// app boundary either. An empty list means this workspace has no HR app, and
/// therefore cannot raise a requisition at all: the cost centre is required.
#[server(name = ChargeableCostCentres, prefix = "/api", endpoint = "inventory/cost-centres")]
pub async fn chargeable_cost_centres()
-> Result<Vec<phonix_ports::cost_centre::CostCentre>, ServerFnError> {
    use crate::state::{pool_and_caller, service_error};

    let (pool, caller) = pool_and_caller().await?;

    phonix_services::inventory::requisition::chargeable(&pool, &caller)
        .await
        .map_err(service_error)
}

#[server(name = RequisitionDetail, prefix = "/api", endpoint = "inventory/requisitions/detail")]
pub async fn requisition_detail(requisition_id: Uuid) -> Result<Requisition, ServerFnError> {
    use crate::state::{pool_and_caller, service_error};

    let (pool, caller) = pool_and_caller().await?;

    phonix_services::inventory::requisition::detail(&pool, &caller, requisition_id)
        .await
        .map_err(service_error)
}

#[server(name = RequisitionEdit, prefix = "/api", endpoint = "inventory/requisitions/edit")]
pub async fn requisition_edit(requisition_id: Uuid) -> Result<RequisitionInput, ServerFnError> {
    use crate::state::{pool_and_caller, service_error};

    let (pool, caller) = pool_and_caller().await?;

    phonix_services::inventory::requisition::edit(&pool, &caller, requisition_id)
        .await
        .map_err(service_error)
}

#[server(name = BlankRequisition, prefix = "/api", endpoint = "inventory/requisitions/blank")]
pub async fn blank_requisition() -> Result<RequisitionInput, ServerFnError> {
    use crate::state::{pool_and_caller, service_error};

    let (_pool, caller) = pool_and_caller().await?;

    phonix_services::inventory::requisition::blank(&caller).map_err(service_error)
}

#[server(name = SaveRequisition, prefix = "/api", endpoint = "inventory/requisitions/save")]
pub async fn save_requisition(
    draft: RequisitionInput,
) -> Result<Submission<RequisitionInput>, ServerFnError> {
    use crate::state::{pool_and_caller, service_error};

    let (pool, caller) = pool_and_caller().await?;

    phonix_services::inventory::requisition::save(&pool, &caller, draft)
        .await
        .map_err(service_error)
}

/// Ask. Allocates the number and puts it in front of whoever answers.
#[server(name = SubmitRequisition, prefix = "/api", endpoint = "inventory/requisitions/submit")]
pub async fn submit_requisition(
    requisition_id: Uuid,
) -> Result<Submission<Requisition>, ServerFnError> {
    use crate::state::{pool_and_caller, service_error};

    let (pool, caller) = pool_and_caller().await?;

    phonix_services::inventory::requisition::submit(&pool, &caller, requisition_id)
        .await
        .map_err(service_error)
}

/// Answer one. `approving` picks which of the two answers; the reason is
/// required either way, and the service is what refuses one without it.
#[server(name = DecideRequisition, prefix = "/api", endpoint = "inventory/requisitions/decide")]
pub async fn decide_requisition(
    requisition_id: Uuid,
    approving: bool,
    decision: DecisionInput,
) -> Result<Submission<Requisition>, ServerFnError> {
    use crate::state::{pool_and_caller, service_error};

    let (pool, caller) = pool_and_caller().await?;

    phonix_services::inventory::requisition::decide(
        &pool,
        &caller,
        requisition_id,
        approving,
        decision,
    )
    .await
    .map_err(service_error)
}

#[server(name = CancelRequisition, prefix = "/api", endpoint = "inventory/requisitions/cancel")]
pub async fn cancel_requisition(requisition_id: Uuid) -> Result<Submission<()>, ServerFnError> {
    use crate::state::{pool_and_caller, service_error};

    let (pool, caller) = pool_and_caller().await?;

    phonix_services::inventory::requisition::cancel(&pool, &caller, requisition_id)
        .await
        .map_err(service_error)
}

#[server(name = DeleteRequisition, prefix = "/api", endpoint = "inventory/requisitions/delete")]
pub async fn delete_requisition(requisition_id: Uuid) -> Result<bool, ServerFnError> {
    use crate::state::{pool_and_caller, service_error};

    let (pool, caller) = pool_and_caller().await?;

    phonix_services::inventory::requisition::delete(&pool, &caller, requisition_id)
        .await
        .map_err(service_error)
}

/// What approved requisitions are still waiting for, grouped by item and place.
///
/// The consolidation screen's query: eleven departments asking for printer paper
/// is one row here, and one line of the order it becomes.
#[server(name = RequisitionDemand, prefix = "/api", endpoint = "inventory/requisitions/demand")]
pub async fn requisition_demand() -> Result<Vec<Demand>, ServerFnError> {
    use crate::state::{pool_and_caller, service_error};

    let (pool, caller) = pool_and_caller().await?;

    phonix_services::inventory::requisition::demand(&pool, &caller)
        .await
        .map_err(service_error)
}

// --- Consolidation -------------------------------------------------------
//
// Eleven departments wanting printer paper, bought once. The screens between
// the requisition and the purchase order: what is waiting, what to buy, and -
// after the fact - which requisitions each order line was raised for.

#[server(name = ListConsolidations, prefix = "/api", endpoint = "inventory/consolidations")]
pub async fn list_consolidations() -> Result<Vec<ConsolidationSummary>, ServerFnError> {
    use crate::state::{pool_and_caller, service_error};

    let (pool, caller) = pool_and_caller().await?;

    phonix_services::inventory::consolidation::list(&pool, &caller)
        .await
        .map_err(service_error)
}

/// What approved requisitions are still waiting for, for the buyer.
///
/// The same view the requisition side exposes, gated on the buyer's permission
/// rather than the requester's - see the service.
#[server(name = ConsolidationDemand, prefix = "/api", endpoint = "inventory/consolidations/demand")]
pub async fn consolidation_demand() -> Result<Vec<Demand>, ServerFnError> {
    use crate::state::{pool_and_caller, service_error};

    let (pool, caller) = pool_and_caller().await?;

    phonix_services::inventory::consolidation::demand(&pool, &caller)
        .await
        .map_err(service_error)
}

#[server(name = ConsolidationDetail, prefix = "/api", endpoint = "inventory/consolidations/detail")]
pub async fn consolidation_detail(consolidation_id: Uuid) -> Result<Consolidation, ServerFnError> {
    use crate::state::{pool_and_caller, service_error};

    let (pool, caller) = pool_and_caller().await?;

    phonix_services::inventory::consolidation::detail(&pool, &caller, consolidation_id)
        .await
        .map_err(service_error)
}

#[server(name = ConsolidationEdit, prefix = "/api", endpoint = "inventory/consolidations/edit")]
pub async fn consolidation_edit(
    consolidation_id: Uuid,
) -> Result<ConsolidationInput, ServerFnError> {
    use crate::state::{pool_and_caller, service_error};

    let (pool, caller) = pool_and_caller().await?;

    phonix_services::inventory::consolidation::edit(&pool, &caller, consolidation_id)
        .await
        .map_err(service_error)
}

/// A consolidation already holding everything one warehouse is waiting for.
///
/// The ordinary way one is started. A blank form is reachable too - the buyer
/// picks the warehouse, and this is what the picker's answer produces.
#[server(name = ConsolidationFromDemand, prefix = "/api", endpoint = "inventory/consolidations/draw")]
pub async fn consolidation_from_demand(
    warehouse_id: Option<Uuid>,
) -> Result<ConsolidationInput, ServerFnError> {
    use crate::state::{pool_and_caller, service_error};

    let (pool, caller) = pool_and_caller().await?;

    match warehouse_id {
        Some(warehouse_id) => {
            phonix_services::inventory::consolidation::from_demand(&pool, &caller, warehouse_id)
                .await
        }
        None => phonix_services::inventory::consolidation::blank(&caller),
    }
    .map_err(service_error)
}

#[server(name = SaveConsolidation, prefix = "/api", endpoint = "inventory/consolidations/save")]
pub async fn save_consolidation(
    draft: ConsolidationInput,
) -> Result<Submission<ConsolidationInput>, ServerFnError> {
    use crate::state::{pool_and_caller, service_error};

    let (pool, caller) = pool_and_caller().await?;

    phonix_services::inventory::consolidation::save(&pool, &caller, draft)
        .await
        .map_err(service_error)
}

/// Raise the orders. One per supplier, each already confirmed.
#[server(name = ConfirmConsolidation, prefix = "/api", endpoint = "inventory/consolidations/confirm")]
pub async fn confirm_consolidation(
    consolidation_id: Uuid,
) -> Result<Submission<Consolidation>, ServerFnError> {
    use crate::state::{pool_and_caller, service_error};

    let (pool, caller) = pool_and_caller().await?;

    phonix_services::inventory::consolidation::confirm(&pool, &caller, consolidation_id)
        .await
        .map_err(service_error)
}

#[server(name = CancelConsolidation, prefix = "/api", endpoint = "inventory/consolidations/cancel")]
pub async fn cancel_consolidation(consolidation_id: Uuid) -> Result<Submission<()>, ServerFnError> {
    use crate::state::{pool_and_caller, service_error};

    let (pool, caller) = pool_and_caller().await?;

    phonix_services::inventory::consolidation::cancel(&pool, &caller, consolidation_id)
        .await
        .map_err(service_error)
}

#[server(name = DeleteConsolidation, prefix = "/api", endpoint = "inventory/consolidations/delete")]
pub async fn delete_consolidation(consolidation_id: Uuid) -> Result<bool, ServerFnError> {
    use crate::state::{pool_and_caller, service_error};

    let (pool, caller) = pool_and_caller().await?;

    phonix_services::inventory::consolidation::delete(&pool, &caller, consolidation_id)
        .await
        .map_err(service_error)
}

/// What each line of a purchase order was raised for.
///
/// A panel on the order screen rather than a screen of its own: the question is
/// only ever asked about an order somebody is already looking at.
#[server(name = OrderAllocation, prefix = "/api", endpoint = "inventory/orders/allocation")]
pub async fn order_allocation(order_id: Uuid) -> Result<Vec<LineAllocation>, ServerFnError> {
    use crate::state::{pool_and_caller, service_error};

    let (pool, caller) = pool_and_caller().await?;

    phonix_services::inventory::consolidation::allocation(&pool, &caller, order_id)
        .await
        .map_err(service_error)
}

// --- Purchase orders -----------------------------------------------------
//
// Nothing here posts. A purchase order commits the workspace to buy something;
// nothing has arrived, nothing is owed, and the accounting starts at the
// receipt below.

#[server(name = ListPurchaseOrders, prefix = "/api", endpoint = "inventory/orders")]
pub async fn list_purchase_orders() -> Result<Vec<OrderSummary>, ServerFnError> {
    use crate::state::{pool_and_caller, service_error};

    let (pool, caller) = pool_and_caller().await?;

    phonix_services::inventory::purchase::list(&pool, &caller)
        .await
        .map_err(service_error)
}

/// What a document line may name: every variant of every item this workspace
/// buys.
#[server(name = PickableVariants, prefix = "/api", endpoint = "inventory/variants/pickable")]
pub async fn pickable_variants() -> Result<Vec<VariantChoice>, ServerFnError> {
    use crate::state::{pool_and_caller, service_error};

    let (pool, caller) = pool_and_caller().await?;

    phonix_services::inventory::purchase::pickable_variants(&pool, &caller)
        .await
        .map_err(service_error)
}

/// The confirmed orders with something still to come, for a receipt to open on.
#[server(name = OrdersAwaitingDelivery, prefix = "/api", endpoint = "inventory/orders/awaiting")]
pub async fn orders_awaiting_delivery() -> Result<Vec<OrderSummary>, ServerFnError> {
    use crate::state::{pool_and_caller, service_error};

    let (pool, caller) = pool_and_caller().await?;

    phonix_services::inventory::purchase::awaiting_delivery(&pool, &caller)
        .await
        .map_err(service_error)
}

#[server(name = PurchaseOrderDetail, prefix = "/api", endpoint = "inventory/orders/detail")]
pub async fn purchase_order_detail(order_id: Uuid) -> Result<PurchaseOrder, ServerFnError> {
    use crate::state::{pool_and_caller, service_error};

    let (pool, caller) = pool_and_caller().await?;

    phonix_services::inventory::purchase::detail(&pool, &caller, order_id)
        .await
        .map_err(service_error)
}

#[server(name = PurchaseOrderEdit, prefix = "/api", endpoint = "inventory/orders/edit")]
pub async fn purchase_order_edit(order_id: Uuid) -> Result<OrderInput, ServerFnError> {
    use crate::state::{pool_and_caller, service_error};

    let (pool, caller) = pool_and_caller().await?;

    phonix_services::inventory::purchase::edit(&pool, &caller, order_id)
        .await
        .map_err(service_error)
}

#[server(name = BlankPurchaseOrder, prefix = "/api", endpoint = "inventory/orders/blank")]
pub async fn blank_purchase_order() -> Result<OrderInput, ServerFnError> {
    use crate::state::{pool_and_caller, service_error};

    let (pool, caller) = pool_and_caller().await?;

    phonix_services::inventory::purchase::blank(&pool, &caller)
        .await
        .map_err(service_error)
}

#[server(name = SavePurchaseOrder, prefix = "/api", endpoint = "inventory/orders/save")]
pub async fn save_purchase_order(
    draft: OrderInput,
) -> Result<Submission<OrderInput>, ServerFnError> {
    use crate::state::{pool_and_caller, service_error};

    let (pool, caller) = pool_and_caller().await?;

    phonix_services::inventory::purchase::save(&pool, &caller, draft)
        .await
        .map_err(service_error)
}

/// Turn a draft into a commitment: allocate its number and freeze the supplier
/// onto it.
#[server(name = ConfirmPurchaseOrder, prefix = "/api", endpoint = "inventory/orders/confirm")]
pub async fn confirm_purchase_order(
    order_id: Uuid,
) -> Result<Submission<PurchaseOrder>, ServerFnError> {
    use crate::state::{pool_and_caller, service_error};

    let (pool, caller) = pool_and_caller().await?;

    phonix_services::inventory::purchase::confirm(&pool, &caller, order_id)
        .await
        .map_err(service_error)
}

#[server(name = CancelPurchaseOrder, prefix = "/api", endpoint = "inventory/orders/cancel")]
pub async fn cancel_purchase_order(order_id: Uuid) -> Result<Submission<()>, ServerFnError> {
    use crate::state::{pool_and_caller, service_error};

    let (pool, caller) = pool_and_caller().await?;

    phonix_services::inventory::purchase::cancel(&pool, &caller, order_id)
        .await
        .map_err(service_error)
}

#[server(name = DeletePurchaseOrder, prefix = "/api", endpoint = "inventory/orders/delete")]
pub async fn delete_purchase_order(order_id: Uuid) -> Result<bool, ServerFnError> {
    use crate::state::{pool_and_caller, service_error};

    let (pool, caller) = pool_and_caller().await?;

    phonix_services::inventory::purchase::delete(&pool, &caller, order_id)
        .await
        .map_err(service_error)
}

// --- Goods receipts ------------------------------------------------------
//
// `post_receipt` is the one endpoint in this module with an accounting
// consequence, and it goes through the `Ledger` port like everything else -
// stock debited, goods-received-not-invoiced credited, per line, in one call
// into the stock ledger.

#[server(name = ListReceipts, prefix = "/api", endpoint = "inventory/receipts")]
pub async fn list_receipts() -> Result<Vec<ReceiptSummary>, ServerFnError> {
    use crate::state::{pool_and_caller, service_error};

    let (pool, caller) = pool_and_caller().await?;

    phonix_services::inventory::receipt::list(&pool, &caller)
        .await
        .map_err(service_error)
}

#[server(name = ReceiptDetail, prefix = "/api", endpoint = "inventory/receipts/detail")]
pub async fn receipt_detail(receipt_id: Uuid) -> Result<Receipt, ServerFnError> {
    use crate::state::{pool_and_caller, service_error};

    let (pool, caller) = pool_and_caller().await?;

    phonix_services::inventory::receipt::detail(&pool, &caller, receipt_id)
        .await
        .map_err(service_error)
}

/// A receipt prefilled with everything an order still owes.
#[server(name = ReceiptAgainstOrder, prefix = "/api", endpoint = "inventory/receipts/against")]
pub async fn receipt_against_order(
    order_id: Uuid,
) -> Result<Submission<ReceiptInput>, ServerFnError> {
    use crate::state::{pool_and_caller, service_error};

    let (pool, caller) = pool_and_caller().await?;

    phonix_services::inventory::receipt::against_order(&pool, &caller, order_id)
        .await
        .map_err(service_error)
}

/// What an order still owes. `None` where nothing is outstanding, which is what
/// a screen draws as complete rather than as an empty list.
#[server(name = OrderBackorder, prefix = "/api", endpoint = "inventory/orders/backorder")]
pub async fn order_backorder(order_id: Uuid) -> Result<Option<Backorder>, ServerFnError> {
    use crate::state::{pool_and_caller, service_error};

    let (pool, caller) = pool_and_caller().await?;

    phonix_services::inventory::receipt::backorder(&pool, &caller, order_id)
        .await
        .map_err(service_error)
}

#[server(name = SaveReceipt, prefix = "/api", endpoint = "inventory/receipts/save")]
pub async fn save_receipt(
    draft: ReceiptInput,
) -> Result<Submission<ReceiptInput>, ServerFnError> {
    use crate::state::{pool_and_caller, service_error};

    let (pool, caller) = pool_and_caller().await?;

    phonix_services::inventory::receipt::save(&pool, &caller, draft)
        .await
        .map_err(service_error)
}

/// Post a receipt: move the stock, value it, and record what is owed for it.
#[server(name = PostReceipt, prefix = "/api", endpoint = "inventory/receipts/post")]
pub async fn post_receipt(receipt_id: Uuid) -> Result<Submission<Receipt>, ServerFnError> {
    use crate::state::{pool_and_caller, service_error};

    let (pool, caller) = pool_and_caller().await?;
    let ledger = phonix_services::books::BooksLedger::new(pool.clone(), caller.clone());

    phonix_services::inventory::receipt::post(&pool, &caller, &ledger, receipt_id)
        .await
        .map_err(service_error)
}

#[server(name = CancelReceipt, prefix = "/api", endpoint = "inventory/receipts/cancel")]
pub async fn cancel_receipt(receipt_id: Uuid) -> Result<Submission<()>, ServerFnError> {
    use crate::state::{pool_and_caller, service_error};

    let (pool, caller) = pool_and_caller().await?;

    phonix_services::inventory::receipt::cancel(&pool, &caller, receipt_id)
        .await
        .map_err(service_error)
}

// --- Supplier bills ------------------------------------------------------
//
// The third document of the three-way match. Posting one clears GRNI, books the
// price difference and creates the payable - all through the `Ledger` port.

#[server(name = ListBills, prefix = "/api", endpoint = "inventory/bills")]
pub async fn list_bills() -> Result<Vec<BillSummary>, ServerFnError> {
    use crate::state::{pool_and_caller, service_error};

    let (pool, caller) = pool_and_caller().await?;

    phonix_services::inventory::bill::list(&pool, &caller)
        .await
        .map_err(service_error)
}

#[server(name = BillDetail, prefix = "/api", endpoint = "inventory/bills/detail")]
pub async fn bill_detail(bill_id: Uuid) -> Result<Bill, ServerFnError> {
    use crate::state::{pool_and_caller, service_error};

    let (pool, caller) = pool_and_caller().await?;

    phonix_services::inventory::bill::detail(&pool, &caller, bill_id)
        .await
        .map_err(service_error)
}

#[server(name = BlankBill, prefix = "/api", endpoint = "inventory/bills/blank")]
pub async fn blank_bill() -> Result<BillInput, ServerFnError> {
    use crate::state::{pool_and_caller, service_error};

    let (pool, caller) = pool_and_caller().await?;

    phonix_services::inventory::bill::blank(&pool, &caller)
        .await
        .map_err(service_error)
}

/// A bill prefilled with everything an order has received and not been billed
/// for.
#[server(name = BillAgainstOrder, prefix = "/api", endpoint = "inventory/bills/against")]
pub async fn bill_against_order(order_id: Uuid) -> Result<Submission<BillInput>, ServerFnError> {
    use crate::state::{pool_and_caller, service_error};

    let (pool, caller) = pool_and_caller().await?;

    phonix_services::inventory::bill::against_order(&pool, &caller, order_id)
        .await
        .map_err(service_error)
}

#[server(name = SaveBill, prefix = "/api", endpoint = "inventory/bills/save")]
pub async fn save_bill(draft: BillInput) -> Result<Submission<BillInput>, ServerFnError> {
    use crate::state::{pool_and_caller, service_error};

    let (pool, caller) = pool_and_caller().await?;

    phonix_services::inventory::bill::save(&pool, &caller, draft)
        .await
        .map_err(service_error)
}

/// How the bill reads against its order and receipts, so the screen can show
/// the reason box before somebody presses post rather than after.
#[server(name = BillMatch, prefix = "/api", endpoint = "inventory/bills/match")]
pub async fn bill_match(bill_id: Uuid) -> Result<MatchGrade, ServerFnError> {
    use crate::state::{pool_and_caller, service_error};

    let (pool, caller) = pool_and_caller().await?;

    phonix_services::inventory::bill::grade(&pool, &caller, bill_id)
        .await
        .map_err(service_error)
}

#[server(name = PostBill, prefix = "/api", endpoint = "inventory/bills/post")]
pub async fn post_bill(
    bill_id: Uuid,
    match_note: Option<String>,
) -> Result<Submission<Bill>, ServerFnError> {
    use crate::state::{pool_and_caller, service_error};

    let (pool, caller) = pool_and_caller().await?;
    let ledger = phonix_services::books::BooksLedger::new(pool.clone(), caller.clone());

    phonix_services::inventory::bill::post(&pool, &caller, &ledger, bill_id, match_note)
        .await
        .map_err(service_error)
}

#[server(name = CancelBill, prefix = "/api", endpoint = "inventory/bills/cancel")]
pub async fn cancel_bill(bill_id: Uuid) -> Result<Submission<()>, ServerFnError> {
    use crate::state::{pool_and_caller, service_error};

    let (pool, caller) = pool_and_caller().await?;

    phonix_services::inventory::bill::cancel(&pool, &caller, bill_id)
        .await
        .map_err(service_error)
}

#[server(name = DeleteBill, prefix = "/api", endpoint = "inventory/bills/delete")]
pub async fn delete_bill(bill_id: Uuid) -> Result<bool, ServerFnError> {
    use crate::state::{pool_and_caller, service_error};

    let (pool, caller) = pool_and_caller().await?;

    phonix_services::inventory::bill::delete(&pool, &caller, bill_id)
        .await
        .map_err(service_error)
}

/// Goods received and not yet billed: the aged GRNI balance.
#[server(name = UnbilledReceipts, prefix = "/api", endpoint = "inventory/bills/unbilled")]
pub async fn unbilled_receipts() -> Result<Vec<UnbilledReceipt>, ServerFnError> {
    use crate::state::{pool_and_caller, service_error};

    let (pool, caller) = pool_and_caller().await?;

    phonix_services::inventory::bill::unbilled(&pool, &caller)
        .await
        .map_err(service_error)
}

// --- Landed costs --------------------------------------------------------
//
// Freight, duty and handling spread over the delivery that carried them. ADR
// 0006 section 6.2. Posting one raises what stock on the shelf is worth and
// charges the rest of it to cost of sales.

#[server(name = ListLandedCosts, prefix = "/api", endpoint = "inventory/landed-costs")]
pub async fn list_landed_costs() -> Result<Vec<LandedCostSummary>, ServerFnError> {
    use crate::state::{pool_and_caller, service_error};

    let (pool, caller) = pool_and_caller().await?;

    phonix_services::inventory::landed_cost::list(&pool, &caller)
        .await
        .map_err(service_error)
}

/// What has been landed on one delivery. The panel on the receipt screen.
#[server(
    name = LandedCostsForReceipt,
    prefix = "/api",
    endpoint = "inventory/landed-costs/for-receipt"
)]
pub async fn landed_costs_for_receipt(
    receipt_id: Uuid,
) -> Result<Vec<LandedCostSummary>, ServerFnError> {
    use crate::state::{pool_and_caller, service_error};

    let (pool, caller) = pool_and_caller().await?;

    phonix_services::inventory::landed_cost::for_receipt(&pool, &caller, receipt_id)
        .await
        .map_err(service_error)
}

/// What one delivery has been landed with in total, from the view that keeps
/// the sum in step. `None` where nothing has been.
#[server(
    name = LandedOnReceipt,
    prefix = "/api",
    endpoint = "inventory/landed-costs/on-receipt"
)]
pub async fn landed_on_receipt(
    receipt_id: Uuid,
) -> Result<Option<ReceiptLandedCost>, ServerFnError> {
    use crate::state::{pool_and_caller, service_error};

    let (pool, caller) = pool_and_caller().await?;

    phonix_services::inventory::landed_cost::landed_on_receipt(&pool, &caller, receipt_id)
        .await
        .map_err(service_error)
}

#[server(
    name = LandedCostDetail,
    prefix = "/api",
    endpoint = "inventory/landed-costs/detail"
)]
pub async fn landed_cost_detail(landed_cost_id: Uuid) -> Result<LandedCost, ServerFnError> {
    use crate::state::{pool_and_caller, service_error};

    let (pool, caller) = pool_and_caller().await?;

    phonix_services::inventory::landed_cost::detail(&pool, &caller, landed_cost_id)
        .await
        .map_err(service_error)
}

#[server(
    name = BlankLandedCost,
    prefix = "/api",
    endpoint = "inventory/landed-costs/blank"
)]
pub async fn blank_landed_cost() -> Result<LandedCostInput, ServerFnError> {
    use crate::state::{pool_and_caller, service_error};

    let (pool, caller) = pool_and_caller().await?;

    phonix_services::inventory::landed_cost::blank(&pool, &caller)
        .await
        .map_err(service_error)
}

#[server(
    name = EditLandedCost,
    prefix = "/api",
    endpoint = "inventory/landed-costs/edit"
)]
pub async fn edit_landed_cost(landed_cost_id: Uuid) -> Result<LandedCostInput, ServerFnError> {
    use crate::state::{pool_and_caller, service_error};

    let (pool, caller) = pool_and_caller().await?;

    phonix_services::inventory::landed_cost::edit(&pool, &caller, landed_cost_id)
        .await
        .map_err(service_error)
}

/// A landed cost against one delivery, which is how the screen is reached from
/// the receipt.
#[server(
    name = LandedCostAgainst,
    prefix = "/api",
    endpoint = "inventory/landed-costs/against"
)]
pub async fn landed_cost_against(
    receipt_id: Uuid,
) -> Result<Submission<LandedCostInput>, ServerFnError> {
    use crate::state::{pool_and_caller, service_error};

    let (pool, caller) = pool_and_caller().await?;

    phonix_services::inventory::landed_cost::against(&pool, &caller, receipt_id)
        .await
        .map_err(service_error)
}

/// What the delivery holds that a charge can be spread over, so the screen can
/// show the cartons before somebody posts rather than after.
#[server(
    name = LandedCostLines,
    prefix = "/api",
    endpoint = "inventory/landed-costs/lines"
)]
pub async fn landed_cost_lines(receipt_id: Uuid) -> Result<Vec<Landable>, ServerFnError> {
    use crate::state::{pool_and_caller, service_error};

    let (pool, caller) = pool_and_caller().await?;

    phonix_services::inventory::landed_cost::landables(&pool, &caller, receipt_id)
        .await
        .map_err(service_error)
}

#[server(
    name = SaveLandedCost,
    prefix = "/api",
    endpoint = "inventory/landed-costs/save"
)]
pub async fn save_landed_cost(
    draft: LandedCostInput,
) -> Result<Submission<LandedCostInput>, ServerFnError> {
    use crate::state::{pool_and_caller, service_error};

    let (pool, caller) = pool_and_caller().await?;

    phonix_services::inventory::landed_cost::save(&pool, &caller, draft)
        .await
        .map_err(service_error)
}

#[server(
    name = PostLandedCost,
    prefix = "/api",
    endpoint = "inventory/landed-costs/post"
)]
pub async fn post_landed_cost(
    landed_cost_id: Uuid,
) -> Result<Submission<LandedCost>, ServerFnError> {
    use crate::state::{pool_and_caller, service_error};

    let (pool, caller) = pool_and_caller().await?;
    let ledger = phonix_services::books::BooksLedger::new(pool.clone(), caller.clone());

    phonix_services::inventory::landed_cost::post(&pool, &caller, &ledger, landed_cost_id)
        .await
        .map_err(service_error)
}

#[server(
    name = CancelLandedCost,
    prefix = "/api",
    endpoint = "inventory/landed-costs/cancel"
)]
pub async fn cancel_landed_cost(landed_cost_id: Uuid) -> Result<Submission<()>, ServerFnError> {
    use crate::state::{pool_and_caller, service_error};

    let (pool, caller) = pool_and_caller().await?;

    phonix_services::inventory::landed_cost::cancel(&pool, &caller, landed_cost_id)
        .await
        .map_err(service_error)
}

#[server(
    name = DeleteLandedCost,
    prefix = "/api",
    endpoint = "inventory/landed-costs/delete"
)]
pub async fn delete_landed_cost(landed_cost_id: Uuid) -> Result<bool, ServerFnError> {
    use crate::state::{pool_and_caller, service_error};

    let (pool, caller) = pool_and_caller().await?;

    phonix_services::inventory::landed_cost::delete(&pool, &caller, landed_cost_id)
        .await
        .map_err(service_error)
}

// --- Stock transfers -----------------------------------------------------
//
// Two movements against one document, with a transit location between them.
// ADR 0006 section 7. Despatch and receive are separate calls because they
// happen at two ends of a road, days apart.

#[server(name = ListTransfers, prefix = "/api", endpoint = "inventory/transfers")]
pub async fn list_transfers() -> Result<Vec<TransferSummary>, ServerFnError> {
    use crate::state::{pool_and_caller, service_error};

    let (pool, caller) = pool_and_caller().await?;

    phonix_services::inventory::transfer::list(&pool, &caller)
        .await
        .map_err(service_error)
}

/// Journeys with stock still on them: what the in-transit account is made of.
#[server(
    name = TransfersInTransit,
    prefix = "/api",
    endpoint = "inventory/transfers/in-transit"
)]
pub async fn transfers_in_transit() -> Result<Vec<TransferSummary>, ServerFnError> {
    use crate::state::{pool_and_caller, service_error};

    let (pool, caller) = pool_and_caller().await?;

    phonix_services::inventory::transfer::in_transit(&pool, &caller)
        .await
        .map_err(service_error)
}

#[server(name = TransferDetail, prefix = "/api", endpoint = "inventory/transfers/detail")]
pub async fn transfer_detail(transfer_id: Uuid) -> Result<Transfer, ServerFnError> {
    use crate::state::{pool_and_caller, service_error};

    let (pool, caller) = pool_and_caller().await?;

    phonix_services::inventory::transfer::detail(&pool, &caller, transfer_id)
        .await
        .map_err(service_error)
}

#[server(name = EditTransfer, prefix = "/api", endpoint = "inventory/transfers/edit")]
pub async fn edit_transfer(transfer_id: Uuid) -> Result<TransferInput, ServerFnError> {
    use crate::state::{pool_and_caller, service_error};

    let (pool, caller) = pool_and_caller().await?;

    phonix_services::inventory::transfer::edit(&pool, &caller, transfer_id)
        .await
        .map_err(service_error)
}

#[server(name = BlankTransfer, prefix = "/api", endpoint = "inventory/transfers/blank")]
pub async fn blank_transfer() -> Result<TransferInput, ServerFnError> {
    use crate::state::{pool_and_caller, service_error};

    let (pool, caller) = pool_and_caller().await?;

    phonix_services::inventory::transfer::blank(&pool, &caller)
        .await
        .map_err(service_error)
}

/// The arrival form, pre-filled with everything still on the road.
#[server(name = TransferArrival, prefix = "/api", endpoint = "inventory/transfers/arrival")]
pub async fn transfer_arrival(
    transfer_id: Uuid,
) -> Result<Submission<ArrivalInput>, ServerFnError> {
    use crate::state::{pool_and_caller, service_error};

    let (pool, caller) = pool_and_caller().await?;

    phonix_services::inventory::transfer::arrival(&pool, &caller, transfer_id)
        .await
        .map_err(service_error)
}

#[server(name = SaveTransfer, prefix = "/api", endpoint = "inventory/transfers/save")]
pub async fn save_transfer(
    draft: TransferInput,
) -> Result<Submission<TransferInput>, ServerFnError> {
    use crate::state::{pool_and_caller, service_error};

    let (pool, caller) = pool_and_caller().await?;

    phonix_services::inventory::transfer::save(&pool, &caller, draft)
        .await
        .map_err(service_error)
}

#[server(name = DespatchTransfer, prefix = "/api", endpoint = "inventory/transfers/despatch")]
pub async fn despatch_transfer(
    transfer_id: Uuid,
) -> Result<Submission<Transfer>, ServerFnError> {
    use crate::state::{pool_and_caller, service_error};

    let (pool, caller) = pool_and_caller().await?;
    let ledger = phonix_services::books::BooksLedger::new(pool.clone(), caller.clone());

    phonix_services::inventory::transfer::despatch(&pool, &caller, &ledger, transfer_id)
        .await
        .map_err(service_error)
}

#[server(name = ReceiveTransfer, prefix = "/api", endpoint = "inventory/transfers/receive")]
pub async fn receive_transfer(
    arrival: ArrivalInput,
) -> Result<Submission<Transfer>, ServerFnError> {
    use crate::state::{pool_and_caller, service_error};

    let (pool, caller) = pool_and_caller().await?;
    let ledger = phonix_services::books::BooksLedger::new(pool.clone(), caller.clone());

    phonix_services::inventory::transfer::receive(&pool, &caller, &ledger, arrival)
        .await
        .map_err(service_error)
}

#[server(name = CancelTransfer, prefix = "/api", endpoint = "inventory/transfers/cancel")]
pub async fn cancel_transfer(transfer_id: Uuid) -> Result<Submission<()>, ServerFnError> {
    use crate::state::{pool_and_caller, service_error};

    let (pool, caller) = pool_and_caller().await?;

    phonix_services::inventory::transfer::cancel(&pool, &caller, transfer_id)
        .await
        .map_err(service_error)
}

#[server(name = DeleteTransfer, prefix = "/api", endpoint = "inventory/transfers/delete")]
pub async fn delete_transfer(transfer_id: Uuid) -> Result<bool, ServerFnError> {
    use crate::state::{pool_and_caller, service_error};

    let (pool, caller) = pool_and_caller().await?;

    phonix_services::inventory::transfer::delete(&pool, &caller, transfer_id)
        .await
        .map_err(service_error)
}

// --- The home page -------------------------------------------------------

/// Items, and how many of them are counted. The gap between the two is the
/// interesting one.
#[server(name = InventoryCounts, prefix = "/api", endpoint = "inventory/counts")]
pub async fn inventory_counts() -> Result<(i64, i64), ServerFnError> {
    use crate::state::{pool_and_caller, service_error};

    let (pool, caller) = pool_and_caller().await?;

    let items = phonix_services::inventory::item::list(&pool, &caller)
        .await
        .map_err(service_error)?;

    let tracked = items.iter().filter(|item| item.is_tracked).count() as i64;

    Ok((items.len() as i64, tracked))
}
