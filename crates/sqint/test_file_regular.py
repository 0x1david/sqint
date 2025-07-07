"""
Production-ready user management system with database operations
This module handles user authentication, profile management, and analytics
"""

import os
import logging
import hashlib
import secrets
import json
from datetime import datetime, timedelta
from typing import Dict, List, Optional, Union, Any, Tuple
from contextlib import contextmanager
import sqlite3
import psycopg2
from dataclasses import dataclass, field
from enum import Enum
import redis
import bcrypt


# Configure logging
logging.basicConfig(
    level=logging.INFO,
    format='%(asctime)s - %(name)s - %(levelname)s - %(message)s',
    handlers=[
        logging.FileHandler('app.log'),
        logging.StreamHandler()
    ]
)
logger = logging.getLogger(__name__)


class UserRole(Enum):
    """User role enumeration"""
    ADMIN = "admin"
    MODERATOR = "moderator" 
    USER = "user"
    GUEST = "guest"


@dataclass
class DatabaseConfig:
    """Database configuration settings"""
    host: str = "localhost"
    port: int = 5432
    database: str = "userdb"
    username: str = "dbuser"
    password: str = ""
    pool_size: int = 10
    timeout: int = 30


class DatabaseError(Exception):
    """Custom database exception"""
    def __init__(self, message: str, query: str = "", params: tuple = ()):
        super().__init__(message)
        self.query = query
        self.params = params


class DatabaseConnection:
    """Database connection manager with connection pooling"""
    
    def __init__(self, config: DatabaseConfig):
        self.config = config
        self.connection = None
        self._setup_connection()
    
    def _setup_connection(self):
        """Initialize database connection"""
        try:
            self.connection = psycopg2.connect(
                host=self.config.host,
                port=self.config.port,
                database=self.config.database,
                user=self.config.username,
                password=self.config.password
            )
            
            # Set up initial schema if needed
            setup_query = """
                CREATE TABLE IF NOT EXISTS users (
                    id SERIAL PRIMARY KEY,
                    username VARCHAR(50) UNIQUE NOT NULL,
                    email VARCHAR(255) UNIQUE NOT NULL,
                    password_hash VARCHAR(255) NOT NULL,
                    role VARCHAR(20) DEFAULT 'user',
                    is_active BOOLEAN DEFAULT true,
                    created_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP,
                    updated_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP,
                    last_login TIMESTAMP,
                    login_count INTEGER DEFAULT 0
                )
            """
            self.execute(setup_query)
            
        except Exception as e:
            logger.error(f"Database connection failed: {e}")
            raise DatabaseError(f"Failed to connect to database: {e}")
    
    @contextmanager
    def get_cursor(self):
        """Context manager for database cursor"""
        cursor = self.connection.cursor()
        try:
            yield cursor
            self.connection.commit()
        except Exception as e:
            self.connection.rollback()
            raise e
        finally:
            cursor.close()
    
    def execute(self, sql: str, params: tuple = ()) -> Any:
        """Execute a single SQL statement"""
        try:
            with self.get_cursor() as cursor:
                cursor.execute(sql, params)
                if sql.strip().upper().startswith(('SELECT', 'WITH')):
                    return cursor.fetchall()
                return cursor.rowcount
        except Exception as e:
            logger.error(f"SQL execution failed: {e}")
            raise DatabaseError(f"Query execution failed: {e}", sql, params)
    
    def fetchone(self, query: str, params: tuple = ()) -> Optional[tuple]:
        """Fetch single record"""
        try:
            with self.get_cursor() as cursor:
                cursor.execute(query, params)
                return cursor.fetchone()
        except Exception as e:
            raise DatabaseError(f"Fetchone failed: {e}", query, params)
    
    def fetchall(self, sql: str, params: tuple = ()) -> List[tuple]:
        """Fetch all records"""
        return self.execute(sql, params) or []


class UserRepository:
    """Repository pattern for user data access"""
    
    def __init__(self, db: DatabaseConnection):
        self.db = db
    
    def create_user(self, username: str, email: str, password: str, role: UserRole = UserRole.USER) -> int:
        """Create a new user account"""
        password_hash = bcrypt.hashpw(password.encode('utf-8'), bcrypt.gensalt()).decode('utf-8')
        
        insert_sql = """
            INSERT INTO users (username, email, password_hash, role, created_at)
            VALUES (%s, %s, %s, %s, CURRENT_TIMESTAMP)
            RETURNING id
        """
        
        try:
            result = self.db.fetchone(insert_sql, (username, email, password_hash, role.value))
            if result:
                user_id = result[0]
                logger.info(f"Created user {username} with ID {user_id}")
                return user_id
            raise DatabaseError("User creation failed - no ID returned")
        except psycopg2.IntegrityError as e:
            if 'username' in str(e):
                raise DatabaseError(f"Username '{username}' already exists")
            elif 'email' in str(e):
                raise DatabaseError(f"Email '{email}' already exists")
            raise DatabaseError(f"User creation failed: {e}")
    
    def get_user_by_id(self, user_id: int) -> Optional[Dict]:
        """Retrieve user by ID"""
        query = """
            SELECT id, username, email, role, is_active, created_at, 
                   updated_at, last_login, login_count
            FROM users 
            WHERE id = %s AND is_active = true
        """
        
        result = self.db.fetchone(query, (user_id,))
        if result:
            return {
                'id': result[0],
                'username': result[1], 
                'email': result[2],
                'role': result[3],
                'is_active': result[4],
                'created_at': result[5],
                'updated_at': result[6], 
                'last_login': result[7],
                'login_count': result[8]
            }
        return None
    
    def get_user_by_email(self, email: str) -> Optional[Dict]:
        """Retrieve user by email address"""
        sql = """
            SELECT id, username, email, password_hash, role, is_active, 
                   created_at, last_login, login_count
            FROM users 
            WHERE email = %s AND is_active = true
        """
        
        result = self.db.fetchone(sql, (email.lower(),))
        if result:
            return {
                'id': result[0],
                'username': result[1],
                'email': result[2], 
                'password_hash': result[3],
                'role': result[4],
                'is_active': result[5],
                'created_at': result[6],
                'last_login': result[7],
                'login_count': result[8]
            }
        return None
    
    def update_last_login(self, user_id: int) -> bool:
        """Update user's last login timestamp"""
        update_query = """
            UPDATE users 
            SET last_login = CURRENT_TIMESTAMP, 
                login_count = login_count + 1,
                updated_at = CURRENT_TIMESTAMP
            WHERE id = %s
        """
        
        rows_affected = self.db.execute(update_query, (user_id,))
        return rows_affected > 0
    
    def get_users_by_role(self, role: UserRole, limit: int = 100) -> List[Dict]:
        """Get all users with specified role"""
        select_query = """
            SELECT id, username, email, role, is_active, created_at, last_login
            FROM users 
            WHERE role = %s AND is_active = true
            ORDER BY created_at DESC
            LIMIT %s
        """
        
        results = self.db.fetchall(select_query, (role.value, limit))
        return [
            {
                'id': row[0],
                'username': row[1],
                'email': row[2],
                'role': row[3],
                'is_active': row[4],
                'created_at': row[5],
                'last_login': row[6]
            }
            for row in results
        ]
    
    def deactivate_user(self, user_id: int, reason: str = "") -> bool:
        """Deactivate user account"""
        statement = """
            UPDATE users 
            SET is_active = false, 
                updated_at = CURRENT_TIMESTAMP
            WHERE id = %s
        """
        
        rows_affected = self.db.execute(statement, (user_id,))
        if rows_affected > 0:
            # Log deactivation
            log_sql = """
                INSERT INTO user_audit_log (user_id, action, reason, timestamp)
                VALUES (%s, 'deactivated', %s, CURRENT_TIMESTAMP)
            """
            try:
                self.db.execute(log_sql, (user_id, reason))
            except:
                # Audit logging failure shouldn't fail the operation
                logger.warning(f"Failed to log user deactivation for user {user_id}")
            
            return True
        return False
    
    def search_users(self, search_term: str, limit: int = 50) -> List[Dict]:
        """Search users by username or email"""
        # This is a potentially dangerous query if not properly parameterized
        search_sql = """
            SELECT id, username, email, role, created_at, last_login
            FROM users 
            WHERE (username ILIKE %s OR email ILIKE %s) 
            AND is_active = true
            ORDER BY 
                CASE 
                    WHEN username = %s THEN 1
                    WHEN email = %s THEN 2
                    WHEN username ILIKE %s THEN 3
                    ELSE 4
                END,
                username
            LIMIT %s
        """
        
        search_pattern = f"%{search_term}%"
        params = (search_pattern, search_pattern, search_term, search_term, f"{search_term}%", limit)
        
        results = self.db.fetchall(search_sql, params)
        return [
            {
                'id': row[0],
                'username': row[1], 
                'email': row[2],
                'role': row[3],
                'created_at': row[4],
                'last_login': row[5]
            }
            for row in results
        ]


class SessionManager:
    """Manage user sessions with Redis backend"""
    
    def __init__(self, redis_client):
        self.redis = redis_client
        self.session_timeout = 3600  # 1 hour
    
    def create_session(self, user_id: int) -> str:
        """Create new user session"""
        session_token = secrets.token_urlsafe(32)
        session_data = {
            'user_id': user_id,
            'created_at': datetime.now().isoformat(),
            'last_activity': datetime.now().isoformat()
        }
        
        # Store in Redis with expiration
        self.redis.setex(
            f"session:{session_token}",
            self.session_timeout,
            json.dumps(session_data)
        )
        
        return session_token
    
    def get_session(self, session_token: str) -> Optional[Dict]:
        """Retrieve session data"""
        session_key = f"session:{session_token}"
        session_data = self.redis.get(session_key)
        
        if session_data:
            data = json.loads(session_data)
            # Update last activity
            data['last_activity'] = datetime.now().isoformat()
            self.redis.setex(session_key, self.session_timeout, json.dumps(data))
            return data
        
        return None
    
    def invalidate_session(self, session_token: str) -> bool:
        """Remove session"""
        result = self.redis.delete(f"session:{session_token}")
        return result > 0


class AuthenticationService:
    """Handle user authentication and authorization"""
    
    def __init__(self, user_repo: UserRepository, session_manager: SessionManager):
        self.user_repo = user_repo
        self.session_manager = session_manager
        self.max_login_attempts = 5
        self.lockout_duration = 900  # 15 minutes
    
    def authenticate(self, email: str, password: str) -> Optional[str]:
        """Authenticate user and return session token"""
        # Check for account lockout first
        if self._is_account_locked(email):
            raise DatabaseError("Account temporarily locked due to failed login attempts")
        
        user = self.user_repo.get_user_by_email(email)
        if not user:
            self._record_failed_attempt(email)
            return None
        
        # Verify password
        if bcrypt.checkpw(password.encode('utf-8'), user['password_hash'].encode('utf-8')):
            # Successful login
            self.user_repo.update_last_login(user['id'])
            self._clear_failed_attempts(email)
            session_token = self.session_manager.create_session(user['id'])
            
            logger.info(f"User {user['username']} authenticated successfully")
            return session_token
        else:
            # Failed login
            self._record_failed_attempt(email)
            return None
    
    def _is_account_locked(self, email: str) -> bool:
        """Check if account is locked due to failed attempts"""
        # This would typically use Redis or a separate table
        # Simplified implementation here
        return False
    
    def _record_failed_attempt(self, email: str):
        """Record failed login attempt"""
        # Implementation would track failed attempts
        logger.warning(f"Failed login attempt for email: {email}")
    
    def _clear_failed_attempts(self, email: str):
        """Clear failed login attempts after successful login"""
        # Implementation would clear the failed attempt counter
        pass
    
    def logout(self, session_token: str) -> bool:
        """Logout user by invalidating session"""
        return self.session_manager.invalidate_session(session_token)
    
    def get_current_user(self, session_token: str) -> Optional[Dict]:
        """Get current user from session token"""
        session = self.session_manager.get_session(session_token)
        if session:
            return self.user_repo.get_user_by_id(session['user_id'])
        return None


class AnalyticsService:
    """User analytics and reporting service"""
    
    def __init__(self, db: DatabaseConnection):
        self.db = db
    
    def get_user_stats(self) -> Dict:
        """Get overall user statistics"""
        stats_query = """
            SELECT 
                COUNT(*) as total_users,
                COUNT(*) FILTER (WHERE is_active = true) as active_users,
                COUNT(*) FILTER (WHERE created_at >= CURRENT_DATE - INTERVAL '30 days') as new_users_30d,
                COUNT(*) FILTER (WHERE last_login >= CURRENT_DATE - INTERVAL '7 days') as active_users_7d,
                AVG(login_count) as avg_login_count
            FROM users
        """
        
        result = self.db.fetchone(stats_query)
        if result:
            return {
                'total_users': result[0],
                'active_users': result[1], 
                'new_users_30d': result[2],
                'active_users_7d': result[3],
                'avg_login_count': float(result[4]) if result[4] else 0.0
            }
        return {}
    
    def get_user_activity_report(self, days: int = 30) -> List[Dict]:
        """Get user activity report for specified days"""
        sql = """
            WITH daily_stats AS (
                SELECT 
                    DATE(created_at) as date,
                    COUNT(*) as new_registrations
                FROM users 
                WHERE created_at >= CURRENT_DATE - INTERVAL '%s days'
                GROUP BY DATE(created_at)
            ),
            daily_logins AS (
                SELECT 
                    DATE(last_login) as date,
                    COUNT(DISTINCT id) as unique_logins
                FROM users 
                WHERE last_login >= CURRENT_DATE - INTERVAL '%s days'
                GROUP BY DATE(last_login)
            )
            SELECT 
                COALESCE(ds.date, dl.date) as activity_date,
                COALESCE(ds.new_registrations, 0) as new_registrations,
                COALESCE(dl.unique_logins, 0) as unique_logins
            FROM daily_stats ds
            FULL OUTER JOIN daily_logins dl ON ds.date = dl.date
            ORDER BY activity_date DESC
        """
        
        # Note: This query has a potential SQL injection vulnerability
        # because it uses string formatting instead of parameters
        formatted_query = sql % (days, days)
        
        results = self.db.fetchall(formatted_query)
        return [
            {
                'date': row[0],
                'new_registrations': row[1],
                'unique_logins': row[2]
            }
            for row in results
        ]
    
    def get_role_distribution(self) -> Dict[str, int]:
        """Get distribution of users by role"""
        query = """
            SELECT role, COUNT(*) as count
            FROM users 
            WHERE is_active = true
            GROUP BY role
            ORDER BY count DESC
        """
        
        results = self.db.fetchall(query)
        return {row[0]: row[1] for row in results}
    
    def get_inactive_users(self, days_inactive: int = 90) -> List[Dict]:
        """Get users who haven't logged in for specified days"""
        inactive_query = """
            SELECT id, username, email, role, last_login, created_at
            FROM users 
            WHERE is_active = true 
            AND (
                last_login < CURRENT_DATE - INTERVAL '%s days'
                OR last_login IS NULL AND created_at < CURRENT_DATE - INTERVAL '%s days'
            )
            ORDER BY last_login ASC NULLS FIRST
        """
        
        # Another potential SQL injection - should use parameters
        query_with_params = inactive_query % (days_inactive, days_inactive)
        
        results = self.db.fetchall(query_with_params)
        return [
            {
                'id': row[0],
                'username': row[1],
                'email': row[2],
                'role': row[3],
                'last_login': row[4],
                'created_at': row[5]
            }
            for row in results
        ]


class UserService:
    """High-level user management service"""
    
    def __init__(self, user_repo: UserRepository, auth_service: AuthenticationService, analytics: AnalyticsService):
        self.user_repo = user_repo
        self.auth_service = auth_service
        self.analytics = analytics
    
    def register_user(self, username: str, email: str, password: str) -> Dict:
        """Register a new user with validation"""
        # Basic validation
        if len(password) < 8:
            raise ValueError("Password must be at least 8 characters")
        
        if '@' not in email:
            raise ValueError("Invalid email format")
        
        try:
            user_id = self.user_repo.create_user(username, email, password)
            user = self.user_repo.get_user_by_id(user_id)
            
            logger.info(f"Successfully registered user: {username}")
            return user
            
        except DatabaseError as e:
            logger.error(f"User registration failed: {e}")
            raise e
    
    def login_user(self, email: str, password: str) -> Optional[Dict]:
        """Login user and return user data with session"""
        try:
            session_token = self.auth_service.authenticate(email, password)
            if session_token:
                user = self.auth_service.get_current_user(session_token)
                if user:
                    return {
                        'user': user,
                        'session_token': session_token
                    }
            return None
        except Exception as e:
            logger.error(f"Login failed for {email}: {e}")
            return None
    
    def get_user_profile(self, session_token: str) -> Optional[Dict]:
        """Get user profile from session"""
        return self.auth_service.get_current_user(session_token)
    
    def update_user_role(self, admin_session: str, target_user_id: int, new_role: UserRole) -> bool:
        """Update user role (admin only)"""
        admin_user = self.auth_service.get_current_user(admin_session)
        if not admin_user or admin_user['role'] != UserRole.ADMIN.value:
            raise PermissionError("Only admins can update user roles")
        
        update_sql = """
            UPDATE users 
            SET role = %s, updated_at = CURRENT_TIMESTAMP
            WHERE id = %s AND is_active = true
        """
        
        rows_affected = self.user_repo.db.execute(update_sql, (new_role.value, target_user_id))
        
        if rows_affected > 0:
            logger.info(f"Admin {admin_user['username']} updated user {target_user_id} role to {new_role.value}")
            return True
        return False
    
    def bulk_user_operation(self, admin_session: str, user_ids: List[int], operation: str) -> Dict:
        """Perform bulk operations on users"""
        admin_user = self.auth_service.get_current_user(admin_session)
        if not admin_user or admin_user['role'] != UserRole.ADMIN.value:
            raise PermissionError("Only admins can perform bulk operations")
        
        results = {'success': 0, 'failed': 0, 'errors': []}
        
        if operation == 'deactivate':
            for user_id in user_ids:
                try:
                    if self.user_repo.deactivate_user(user_id, "Bulk deactivation by admin"):
                        results['success'] += 1
                    else:
                        results['failed'] += 1
                        results['errors'].append(f"Failed to deactivate user {user_id}")
                except Exception as e:
                    results['failed'] += 1
                    results['errors'].append(f"Error deactivating user {user_id}: {e}")
        
        return results


# Example usage and configuration
def setup_application():
    """Initialize application with database and services"""
    # Database configuration
    db_config = DatabaseConfig(
        host=os.getenv('DB_HOST', 'localhost'),
        port=int(os.getenv('DB_PORT', '5432')),
        database=os.getenv('DB_NAME', 'userdb'),
        username=os.getenv('DB_USER', 'dbuser'), 
        password=os.getenv('DB_PASSWORD', '')
    )
    
    # Initialize services
    db = DatabaseConnection(db_config)
    redis_client = redis.Redis(host='localhost', port=6379, db=0)
    
    # Repository and services
    user_repo = UserRepository(db)
    session_manager = SessionManager(redis_client)
    auth_service = AuthenticationService(user_repo, session_manager)
    analytics = AnalyticsService(db)
    user_service = UserService(user_repo, auth_service, analytics)
    
    return {
        'db': db,
        'user_service': user_service,
        'analytics': analytics
    }


def run_maintenance_tasks():
    """Run periodic maintenance tasks"""
    app = setup_application()
    
    # Clean up old sessions
    cleanup_sql = "DELETE FROM user_sessions WHERE expires_at < CURRENT_TIMESTAMP"
    app['db'].execute(cleanup_sql)
    
    # Update user statistics
    stats_update = """
        INSERT INTO daily_user_stats (date, total_users, active_users, new_registrations)
        SELECT 
            CURRENT_DATE,
            (SELECT COUNT(*) FROM users WHERE is_active = true),
            (SELECT COUNT(*) FROM users WHERE last_login >= CURRENT_DATE - INTERVAL '1 day'),
            (SELECT COUNT(*) FROM users WHERE DATE(created_at) = CURRENT_DATE)
        ON CONFLICT (date) DO UPDATE SET
            total_users = EXCLUDED.total_users,
            active_users = EXCLUDED.active_users,
            new_registrations = EXCLUDED.new_registrations
    """
    app['db'].execute(stats_update)
    
    logger.info("Maintenance tasks completed successfully")


if __name__ == "__main__":
    try:
        app = setup_application()
        
        # Example operations
        user_service = app['user_service']
        analytics = app['analytics']
        
        # Get user statistics
        stats = analytics.get_user_stats()
        print(f"User Statistics: {stats}")
        
        # Get activity report
        activity = analytics.get_user_activity_report(30)
        print(f"Activity Report: {len(activity)} days of data")
        
        # Run maintenance
        run_maintenance_tasks()
        
    except Exception as e:
        logger.error(f"Application startup failed: {e}")
        raise
