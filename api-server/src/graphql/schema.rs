use crate::{
    auth::auth::{Auth, RoleGuard, ROLE_CUSTOMER, ROLE_SUPPLIER},
    entity::sea_orm_active_enums::UserRole,
    models::{
        products::{Categories, Products},
        user::{
            Customers, LoginUser, RegisterCustomer, RegisterSupplier, RegisterUser, Suppliers,
            Users,
        },
    },
};
use async_graphql::{http::GraphiQLSource, Context, EmptySubscription, Object, Schema};
use axum::response::{self, IntoResponse};
use sea_orm::{
    ActiveEnum, ActiveValue::Set, ColumnTrait, DatabaseConnection, EntityTrait, QueryFilter,
};

macro_rules! role_guard {
    ($($role:expr),*) => {
        RoleGuard::new(vec![$($role),*])
    };
}

pub struct QueryRoot;
pub struct MutationRoot;

#[Object]
impl QueryRoot {
    async fn products_with_id(
        &self,
        ctx: &Context<'_>,
        category_id: Option<i32>,
        supplier_id: Option<i32>,
        base_product_id: Option<i32>,
    ) -> Result<Vec<Products>, async_graphql::Error> {
        use crate::entity::products;
        let db = ctx.data::<DatabaseConnection>()?;

        let products = products::Entity::find()
            .filter(match (category_id, supplier_id, base_product_id) {
                (Some(category_id), None, None) => products::Column::CategoryId.eq(category_id),
                (None, Some(supplier_id), None) => products::Column::SupplierId.eq(supplier_id),
                (None, None, Some(base_product_id)) => {
                    products::Column::BaseProductId.eq(base_product_id)
                }
                _ => products::Column::CategoryId
                    .eq(category_id)
                    .and(products::Column::SupplierId.eq(supplier_id))
                    .and(products::Column::BaseProductId.eq(base_product_id)),
            })
            .all(db)
            .await?;

        let products: Vec<Products> = products.into_iter().map(|product| product.into()).collect();

        Ok(products)
    }

    async fn products_with_name(
        &self,
        ctx: &Context<'_>,
        name: String,
    ) -> Result<Vec<Products>, async_graphql::Error> {
        use crate::entity::products;
        let db = ctx.data::<DatabaseConnection>()?;

        let products = products::Entity::find()
            .filter(products::Column::Name.contains(name))
            .all(db)
            .await?;

        let products: Vec<Products> = products.into_iter().map(|product| product.into()).collect();

        Ok(products)
    }

    async fn categories(&self, ctx: &Context<'_>) -> Result<Vec<Categories>, async_graphql::Error> {
        use crate::entity::categories;
        let db = ctx.data::<DatabaseConnection>()?;

        let categories = categories::Entity::find().all(db).await?;

        let categories: Vec<Categories> = categories
            .into_iter()
            .map(|category| Categories {
                category_id: category.category_id,
                name: category.name,
                parent_category_id: category.parent_category_id,
            })
            .collect();

        Ok(categories)
    }

    #[graphql(guard = "role_guard!(ROLE_CUSTOMER, ROLE_SUPPLIER)")]
    async fn get_user(
        &self,
        ctx: &Context<'_>,
        token: String,
    ) -> Result<Users, async_graphql::Error> {
        use crate::entity::users;
        let db = ctx.data::<DatabaseConnection>()?;

        let user = users::Entity::find()
            .filter(users::Column::UserId.eq(Auth::verify_token(&token)?.user_id))
            .one(db)
            .await
            .map_err(|_| "User not found")?
            .map(|user| user.into())
            .unwrap();

        Ok(user)
    }

    #[graphql(guard = "role_guard!(ROLE_CUSTOMER)")]
    async fn customer_profile(
        &self,
        ctx: &Context<'_>,
        token: String,
    ) -> Result<Customers, async_graphql::Error> {
        use crate::entity::customers;
        let db = ctx.data::<DatabaseConnection>()?;

        let customer = customers::Entity::find()
            .filter(customers::Column::UserId.eq(Auth::verify_token(&token)?.user_id))
            .one(db)
            .await
            .map_err(|_| "Customer not found")?
            .map(|customer| customer.into())
            .unwrap();

        Ok(customer)
    }

    #[graphql(guard = "role_guard!(ROLE_SUPPLIER)")]
    async fn supplier_profile(
        &self,
        ctx: &Context<'_>,
        token: String,
    ) -> Result<Suppliers, async_graphql::Error> {
        use crate::entity::suppliers;
        let db = ctx.data::<DatabaseConnection>()?;

        let supplier = suppliers::Entity::find()
            .filter(suppliers::Column::UserId.eq(Auth::verify_token(&token)?.user_id))
            .one(db)
            .await
            .map_err(|_| "Supplier not found")?
            .map(|supplier| supplier.into())
            .unwrap();

        Ok(supplier)
    }
}

#[Object]
impl MutationRoot {
    async fn register_user(
        &self,
        ctx: &Context<'_>,
        input: RegisterUser,
    ) -> Result<String, async_graphql::Error> {
        use crate::entity::users;

        if users::Entity::find()
            .filter(users::Column::Email.eq(&input.email))
            .one(ctx.data::<DatabaseConnection>()?)
            .await?
            .is_some()
        {
            return Err("User already exists".into());
        }

        let db = ctx.data::<DatabaseConnection>()?;

        let role = match input.role.as_str() {
            ROLE_CUSTOMER => UserRole::Customer,
            ROLE_SUPPLIER => UserRole::Supplier,
            _ => return Err("Invalid role".into()),
        };

        let password;
        match Auth::check_password_strength(&input.password) {
            Ok(_) => password = Auth::hash_password(&input.password)?,
            Err(e) => return Err(e.into()),
        }

        let user = users::ActiveModel {
            email: Set(input.email),
            password: Set(password),
            role: Set(role),
            ..Default::default()
        };
        let insert_user = users::Entity::insert(user).exec_with_returning(db).await?;

        Ok(Auth::create_token(
            insert_user.user_id,
            insert_user.role.to_value(),
        )?)
    }

    #[graphql(guard = "role_guard!(ROLE_CUSTOMER)")]
    async fn register_customer(
        &self,
        ctx: &Context<'_>,
        input: RegisterCustomer,
        token: String,
    ) -> Result<Customers, async_graphql::Error> {
        use crate::entity::customers;

        let db = ctx.data::<DatabaseConnection>()?;

        let customer = customers::ActiveModel {
            first_name: Set(input.first_name),
            last_name: Set(input.last_name),
            user_id: Set(Auth::verify_token(&token)?.user_id.parse::<i32>()?),
            ..Default::default()
        };

        let insert_customer = customers::Entity::insert(customer)
            .exec_with_returning(db)
            .await?;

        Ok(insert_customer.into())
    }

    #[graphql(guard = "role_guard!(ROLE_SUPPLIER)")]
    async fn register_supplier(
        &self,
        ctx: &Context<'_>,
        input: RegisterSupplier,
        token: String,
    ) -> Result<Suppliers, async_graphql::Error> {
        use crate::entity::suppliers;

        let db = ctx.data::<DatabaseConnection>()?;

        let supplier = suppliers::ActiveModel {
            user_id: Set(Auth::verify_token(&token)?.user_id.parse::<i32>()?),
            contact_phone: Set(input.contact_phone),
            ..Default::default()
        };

        let insert_supplier = suppliers::Entity::insert(supplier)
            .exec_with_returning(db)
            .await?;

        Ok(insert_supplier.into())
    }

    async fn login(
        &self,
        ctx: &Context<'_>,
        login_details: LoginUser,
    ) -> Result<String, async_graphql::Error> {
        use crate::entity::users;

        let db = ctx.data::<DatabaseConnection>()?;

        let user: Users = users::Entity::find()
            .filter(users::Column::Email.eq(&login_details.email))
            .one(db)
            .await
            .map_err(|_| "User not found")?
            .map(|user| user.into())
            .unwrap();

        match Auth::verify_password(&login_details.password, &user.password) {
            Ok(verification_status) => {
                if verification_status {
                    Ok(Auth::create_token(user.user_id, user.role)?)
                } else {
                    Err("Invalid password".into())
                }
            }
            Err(_) => Err("Password not readable, please reset password".into()),
        }
    }
}

pub type AppSchema = Schema<QueryRoot, MutationRoot, EmptySubscription>;

pub fn create_schema(db: DatabaseConnection, redis: redis::Client) -> AppSchema {
    Schema::build(QueryRoot, MutationRoot, EmptySubscription)
        .data(db)
        .data(redis)
        .finish()
}

pub async fn graphiql() -> impl IntoResponse {
    response::Html(GraphiQLSource::build().endpoint("/").finish())
}
