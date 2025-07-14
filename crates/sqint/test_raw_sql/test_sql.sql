-- Simple SELECT query
SELECT id, name, email 
FROM users 
WHERE active = 1
ORDER BY name;

-- Query with JOIN
SELECT u.name, p.title, p.created_at
FROM users u
INNER JOIN posts p ON u.id = p.user_id
WHERE p.status = 'published'
AND u.active = 1;

-- INSERT statement
INSERT INTO products (name, price, category_id, description)
VALUES ('Laptop', 999.99, 1, 'High-performance laptop for professionals');

-- Query with syntax error
SELECT name, email, created_at <>
WHERE user_id = 42
AND status = 'active';

-- UPDATE statement
UPDATE users 
SET last_login = NOW(), 
    login_count = login_count + 1
WHERE id = 123;

-- Complex query with subquery
SELECT 
    category_name,
    product_count,
    avg_price
FROM (
    SELECT 
        c.name as category_name,
        COUNT(p.id) as product_count,
        AVG(p.price) as avg_price
    FROM categories c
    LEFT JOIN products p ON c.id = p.category_id
    GROUP BY c.id, c.name
) subquery
WHERE product_count > 0
ORDER BY avg_price DESC;

-- DELETE statement
DELETE FROM temp_logs 
WHERE created_at < DATE_SUB(NOW(), INTERVAL 30 DAY);
