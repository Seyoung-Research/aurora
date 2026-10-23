# Regression and Curve Fitting (`src/stats/regression.rs`)

The `regression` module provides tools for modeling the relationship between variables, ranging from simple linear fits to complex non-linear curves.

## Linear Models

### Ordinary Least Squares (OLS)
Implements `simple_linear_regression` to fit $y = \beta_0 + \beta_1 x$. It computes the slope, intercept, and standard errors.

### Deming Regression
Provides `deming_regression` for "errors-in-variables" models. Unlike OLS, Deming regression accounts for measurement errors in both the X and Y variables, using a variance ratio $\lambda$.

## Non-Linear Curve Fitting

Aurora implements a robust non-linear least squares engine based on the **Levenberg-Marquardt (LM)** algorithm.

### The LM Pipeline
1. **Model Definition**: `ModelDef` defines a function $f(x, p)$, its parameter names, and initial guesses.
2. **Jacobian Estimation**: Since models can be arbitrary, the Jacobian is computed numerically via central differences.
3. **Adaptive Damping**: The LM algorithm adaptively switches between the Gauss-Newton method and Gradient Descent to ensure convergence even from poor initial guesses.
4. **Solving**: Uses Gaussian elimination with partial pivoting to solve the normal equations.

### Built-in Model Library
The `model_library()` provides several common scientific models:
- **Sigmoidal Dose-Response** (4PL)
- **Exponential Growth/Decay**
- **Michaelis-Menten**
- **Gompertz**
- **Logarithmic and Power** fits.

## Curve Utilities
- **Trapezoidal AUC**: Computes the Area Under the Curve.
- **LOWESS**: Implements Locally Weighted Scatterplot Smoothing.
- **Cubic Splines**: Natural cubic spline interpolation for smooth data representation.
