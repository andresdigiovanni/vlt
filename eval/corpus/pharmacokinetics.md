---
title: Pharmacokinetics
source: https://en.wikipedia.org/wiki/Pharmacokinetics
revision: 1373279536
license: CC BY-SA 4.0
---

# Pharmacokinetics

**Pharmacokinetics** (from Ancient Greek *pharmakon* 'drug' and *kinetikos* 'moving, putting in motion'; see chemical kinetics), sometimes abbreviated as **PK**, is a branch of pharmacology dedicated to describing how the body affects a specific substance after administration. The substances of interest include any chemical xenobiotics such as pharmaceutical drugs, pesticides, food additives, cosmetics, etc. PK attempts to analyze chemical metabolism and discover the fate of a chemical from the moment that it is administered up to the point at which it is completely eliminated from the body. PK is based on mathematical modeling that places great emphasis on the relationship between drug plasma concentration and the time elapsed since the drug's administration. Pharmacokinetics is the study of how an organism affects the drug, whereas [[pharmacodynamics]] (PD) is the study of how the drug affects the organism. Both together influence dosing, benefit, and adverse effects, as seen in PK/PD models.

IUPAC definition

> **Pharmacokinetics**:
>
> 1. Process of the uptake of drugs by the body, the biotransformation they undergo, the distribution of the drugs and their metabolites in the tissues, and the elimination of the drugs and their metabolites from the body over a period of time.
> 2. Study of more such related processes

## ADME

A number of phases occur once the drug enters into contact with the organism, these are described using the acronym ADME (or LADME if liberation is included as a separate step from absorption):

- **L**iberation – the process of active pharmaceutical ingredients (API) separating from its pharmaceutical formulation. See also IVIVC.
- **A**bsorption – the process of a drug entering into systemic circulation from the site of administration.
- **D**istribution – the dispersion or dissemination of substances throughout the fluids and tissues of the body.
- **M**etabolism (or biotransformation, or inactivation) – the chemical reactions of the drug and irreversible breakdown into metabolites (e.g. by metabolic enzymes such as cytochrome P450 or glucuronosyltransferase enzymes).
- **E**xcretion – the removal of the substance or metabolites from the body. In rare cases, some drugs irreversibly accumulate in body tissue.

Some textbooks combine the first two phases as the drug is often administered in an active form, which means that there is no liberation phase. Others include a phase that combines distribution, metabolism and excretion into a disposition phase. Other authors include the drug's toxicological aspect in what is known as *ADME-Tox* or *ADMET*. The two phases of metabolism and excretion can be grouped together under the title elimination.

The study of these distinct phases involves the use and manipulation of basic concepts in order to understand the process dynamics. For this reason, in order to fully comprehend the *kinetics* of a drug, it is necessary to have detailed knowledge of a number of factors such as: the properties of the substances that act as excipients, the characteristics of the appropriate biological membranes and how various substances can cross them, or how several enzyme reactions may induce or inhibit the drug.

## Metrics

The following are the most commonly measured pharmacokinetic metrics: The units of the dose in the table are expressed in moles (mol) and molar (M). To express the metrics of the table in units of mass, instead of amount of substance, simply replace 'mol' with 'g' and 'M' with 'g/L'. Similarly, other units in the table may be expressed in units of an equivalent dimension by scaling.

| Characteristic | Description | Symbol | Unit | Formula | Worked example value |
| --- | --- | --- | --- | --- | --- |
| Dose | Amount of drug administered. | `D` | `\mathrm {mol}` | Design parameter | 500 mmol |
| Dosing interval | Time interval between drug dose administrations. | `\tau` | `\mathrm {h}` | Design parameter | 24 h |
| Maximum serum concentration | The peak plasma concentration of a drug after administration. | `C_{\text{max}}` | `\mathrm {mmol/L}` | Direct measurement | 60.9 mmol/L |
| Minimum time for `C_{\text{max}}` | Minimum time taken to reach `C_{\text{max}}`. | `t_{\text{max}}` | `\mathrm {h}` | Direct measurement | 3.9 h |
| Minimum plasma concentration | The lowest (trough) concentration that a drug reaches before the next dose is administered. | `C_{{\text{min}},{\text{ss}}}` | `\mathrm {mmol/L}` | `C_{\text{min}}={\frac {SFDk_{a}}{V_{d}(k_{a}-k_{e})}}\times \left({\frac {e^{-k_{e}\tau }}{1-e^{-k_{e}\tau }}}-{\frac {e^{-k_{a}\tau }}{1-e^{-k_{a}\tau }}}\right)` | 27.7 mmol/L |
| Average plasma concentration | The average plasma concentration of a drug over the dosing interval in steady state. | `C_{{\text{av}},{\text{ss}}}` | `\mathrm {h\times mmol/L}` | `{\frac {AUC_{\tau ,{\text{ss}}}}{\tau }}` | 55.0 h×mmol/L |
| Volume of distribution | The apparent volume in which a drug is distributed (i.e., the parameter relating drug concentration in plasma to drug amount in the body). | `V_{\text{d}}` | `\mathrm {L}` | `{\frac {D}{C_{0}}}` | 6.0 L |
| Concentration | Amount of drug in a given volume of plasma. | `C_{0},C_{\text{ss}}` | `\mathrm {mmol/L}` | `{\frac {D}{V_{\text{d}}}}` | 83.3 mmol/L |
| Absorption half-life | The time required for 50% of a given dose of drug to be absorbed into the systemic circulation. | `t_{{\frac {1}{2}}a}` | `\mathrm {h}` | `{\frac {\ln(2)}{k_{\text{a}}}}` | 1.0 h |
| Absorption rate constant | The rate at which a drug enters into the body for oral and other extravascular routes. | `k_{\text{a}}` | `\mathrm {h} ^{-1}` | `{\frac {\ln(2)}{t_{{\frac {1}{2}}a}}}` | 0.693 h−1 |
| Elimination half-life | The time required for the concentration of the drug to reach half of its original value. | `t_{{\frac {1}{2}}b}` | `\mathrm {h}` | `{\frac {\ln(2)}{k_{\text{e}}}}` | 12 h |
| Elimination rate constant | The rate at which a drug is removed from the body. | `k_{\text{e}}` | `\mathrm {h} ^{-1}` | `{\frac {\ln(2)}{t_{{\frac {1}{2}}b}}}={\frac {CL}{V_{\text{d}}}}` | 0.0578 h−1 |
| Infusion rate | Rate of infusion required to balance elimination. | `k_{\text{in}}` | `\mathrm {mol/h}` | `C_{\text{ss}}\cdot CL` | 50 mmol/h |
| Area under the curve | The integral of the concentration-time curve (after a single dose or in steady state). | `AUC_{0-\infty }` | `\mathrm {M} \cdot \mathrm {s}` | `\int _{0}^{\infty }C\,\mathrm {d} t` | 1,320 h×mmol/L |
| `AUC_{\tau ,{\text{ss}}}` | `\mathrm {M} \cdot \mathrm {s}` | `\int _{t}^{t+\tau }C\,\mathrm {d} t` |
| Clearance | The volume of plasma cleared of the drug per unit time. | `CL` | `\mathrm {m} ^{3}/\mathrm {s}` | `V_{\text{d}}\cdot k_{\text{e}}={\frac {D}{AUC}}` | 0.38 L/h |
| Bioavailability | The systemically available fraction of a drug. | `f` | Unitless | `{\frac {AUC_{\text{po}}\cdot D_{\text{iv}}}{AUC_{\text{iv}}\cdot D_{\text{po}}}}` | 0.8 |
| Fluctuation | Peak–trough fluctuation within one dosing interval at steady state. | `\%PTF` | `\%` | `100{\frac {C_{{\text{max}},{\text{ss}}}-C_{{\text{min}},{\text{ss}}}}{C_{{\text{av}},{\text{ss}}}}}` where `C_{{\text{av}},{\text{ss}}}={\frac {AUC_{\tau ,{\text{ss}}}}{\tau }}` | 41.8% |

In pharmacokinetics, *steady state* refers to the situation where the overall intake of a drug is fairly in dynamic equilibrium with its elimination. In practice, it is generally considered that once regular dosing of a drug is started, steady state is reached after 3 to 5 times its half-life. In steady state and in linear pharmacokinetics, AUCτ=AUC∞.

## Modeling

Models have been developed to simplify conceptualization of the many processes that take place in the interaction between an organism and a chemical substance. Pharmacokinetic modelling may be performed either by noncompartmental or compartmental methods. Multi-compartment models provide the best approximations to reality; however, the complexity involved in adding parameters with that modelling approach means that *monocompartmental models* and above all *two compartmental models* are the most-frequently used. The model outputs for a drug can be used in industry (for example, in calculating bioequivalence when designing [[generic-drug|generic drugs]]) or in the clinical application of pharmacokinetic concepts. Clinical pharmacokinetics provides many performance guidelines for effective and efficient use of drugs for human-health professionals and in veterinary medicine.

Models generally take the form of mathematical formulas that have a corresponding graphical representation. The use of these models allows an understanding of the characteristics of a molecule, as well as how a particular drug will behave given information regarding some of its basic characteristics such as its acid dissociation constant (pKa), [[bioavailability]] and solubility, absorption capacity and distribution in the organism. A variety of analysis techniques may be used to develop models, such as nonlinear regression or curve stripping.

### Noncompartmental analysis

Noncompartmental methods estimate PK parameters directly from a table of concentration-time measurements. Noncompartmental methods are versatile in that they do not assume any specific model and generally produce accurate results acceptable for bioequivalence studies. Total drug exposure is most often estimated by area under the curve (AUC) methods, with the trapezoidal rule (numerical integration) the most common method. Due to the dependence on the length of *x* in the trapezoidal rule, the area estimation is highly dependent on the blood/plasma sampling schedule. That is, the closer time points are, the closer the trapezoids reflect the actual shape of the concentration-time curve. The number of time points available in order to perform a successful NCA analysis should be enough to cover the absorption, distribution and elimination phase to accurately characterize the drug. Beyond AUC exposure measures, parameters such as Cmax (maximum concentration), Tmax (time to maximum concentration), CL and Vd can also be reported using NCA methods.

### Compartmental analysis

Compartment models methods estimate the concentration-time graph by modeling it as a system of differential equations. These models are based on a consideration of an organism as a number of related *compartments*. Both single compartment and multi-compartment models are in use. PK compartmental models are often similar to kinetic models used in other scientific disciplines such as chemical kinetics and thermodynamics. The advantage of compartmental over noncompartmental analysis is the ability to modify parameters and to extrapolate to novel situations. The disadvantage is the difficulty in developing and validating the proper model. Although compartment models have the potential to realistically model the situation within an organism, models inevitably make simplifying assumptions and will not be applicable in all situations. However complicated and precise a model may be, it still does not truly represent reality despite the effort involved in obtaining various distribution values for a drug. This is because the concept of distribution volume is a relative concept that is not a true reflection of reality. The choice of model therefore comes down to deciding which one offers the lowest margin of error for the drug involved.

#### Single-compartment model

The simplest PK compartmental model is the one-compartmental PK model. This models an organism as one homogenous compartment. This *monocompartmental model* presupposes that blood plasma concentrations of the drug are the only information needed to determine the drug's concentration in other fluids and tissues. For example, the concentration in other areas may be approximately related by known, constant factors to the blood plasma concentration.

In this one-compartment model, the most common model of elimination is first order kinetics, where the elimination of the drug is directly proportional to the drug's concentration in the organism. This is often called *linear pharmacokinetics*, as the change in concentration over time can be expressed as a linear differential equation `{\frac {dC}{dt}}=-k_{\text{el}}C`. Assuming a single IV bolus dose resulting in a concentration `C_{\text{initial}}` at time `t=0`, the equation can be solved to give `C=C_{\text{initial}}\times e^{-k_{\text{el}}\times t}`.

#### Two-compartment model

Not all body tissues have the same blood supply, so the distribution of the drug will be slower in those tissues than in others with a better blood supply. Furthermore, there are some tissues (such as the brain tissue) that present a real barrier to the distribution of drugs, which may be breached with greater or lesser ease depending on the drug's characteristics. If these relative conditions for the different tissue types are considered along with the rate of elimination, the organism can be considered to be acting like two compartments: one that we can call the *central compartment,* which has a more rapid distribution and consists of organs and systems with a well-developed blood supply; and the *peripheral compartment,* which is made up of organs with a lower blood flow. Other tissues, such as the brain, can occupy a variable position depending on a drug's ability to passively transport (high lipophilicity) and evade active efflux to cross the blood**–**brain barrier (BBB) that separates the organ from the blood supply.

Two-compartment models vary depending on which compartment elimination occurs in. The most common situation is that elimination occurs in the central compartment as the liver and kidneys are organs with a good blood supply. However, in some situations, elimination occurs in the peripheral compartment or even in both compartments. This can mean that there are three possible variations in the two compartment model, which still do not cover all possibilities.

#### Multi-compartment models

In the real world, each tissue will have its own distribution characteristics and none of them will be strictly linear. The two-compartment model may not be applicable in situations where some of the enzymes responsible for metabolizing the drug become saturated, or where an active elimination mechanism is present that is independent of the drug's plasma concentration. If we label the drug's volume of distribution within the organism **VdF** and its volume of distribution in a tissue **VdT** the former will be described by an equation that takes into account all the tissues that act in different ways, that is:

`Vd_{F}=Vd_{T1}+Vd_{T2}+Vd_{T3}+\cdots +Vd_{Tn}\,`

This represents the *multi-compartment model* with a number of curves that express complicated equations in order to obtain an overall curve. A number of computer programs have been developed to plot these equations. The most complex PK models (called PBPK models) rely on the use of physiological information to ease development and validation.

The graph for the non-linear relationship between the various factors is represented by a curve; the relationships between the factors can then be found by calculating the dimensions of different areas under the curve. The models used in *non-linear pharmacokinetics* are largely based on Michaelis–Menten kinetics. A reaction's factors of non-linearity include the following:

- Multiphasic absorption: Drugs injected intravenously are removed from the plasma through two primary mechanisms: (1) Distribution to body tissues and (2) metabolism + excretion of the drugs. The resulting decrease of the drug's plasma concentration follows a biphasic pattern (see figure).
  - Alpha phase: An initial phase of rapid decrease in plasma concentration. The decrease is primarily attributed to drug distribution from the central compartment (circulation) into the peripheral compartments (body tissues). This phase ends when a pseudo-equilibrium of drug concentration is established between the central and peripheral compartments.
  - Beta phase: A phase of gradual decrease in plasma concentration after the alpha phase. The decrease is primarily attributed to drug elimination, that is, metabolism and excretion.
  - Additional phases (gamma, delta, etc.) are sometimes seen.
- A drug's characteristics make a clear distinction between tissues with high and low blood flow.
- Enzymatic saturation: When the dose of a drug whose elimination depends on biotransformation is increased above a certain threshold the enzymes responsible for its metabolism become saturated. The drug's plasma concentration will then increase disproportionately and its elimination will no longer be constant.
- Induction or enzymatic inhibition: Some drugs have the capacity to inhibit or stimulate their own metabolism, in negative or positive feedback reactions (e.g. this occurs with fluvoxamine, fluoxetine and phenytoin). As larger doses of these pharmaceuticals are administered the plasma concentrations of the unmetabolized drug increases and the [[biological-half-life|elimination half-life]] increases. It is therefore necessary to adjust the dose or other treatment parameters when a high dosage is required.
- The kidneys can also establish active elimination mechanisms for some drugs, independent of plasma concentrations.

It can therefore be seen that non-linearity can occur because of reasons that affect the entire pharmacokinetic sequence: absorption, distribution, metabolism and elimination.

## Bioavailability

At a practical level, a drug's [[bioavailability]] can be defined as the proportion of the drug that reaches the systemic circulation. From this perspective the intravenous administration of a drug provides the greatest possible bioavailability, and this method is considered to yield a bioavailability of 1 (or 100%). Bioavailability of other delivery methods is compared with that of intravenous injection (absolute bioavailability) or to a standard value related to other delivery methods in a particular study (relative bioavailability).

`B_{A}={\frac {[ABC]_{P}\cdot D_{IV}}{[ABC]_{IV}\cdot D_{P}}}`

`{\mathit {B}}_{R}={\frac {[ABC]_{A}\cdot {\text{dose}}_{B}}{[ABC]_{B}\cdot {\text{dose}}_{A}}}`

Once a drug's bioavailability has been established it is possible to calculate the changes that need to be made to its dosage in order to reach the required blood plasma levels. Bioavailability is, therefore, a mathematical factor for each individual drug that influences the administered dose. It is possible to calculate the amount of a drug in the blood plasma that has a real potential to bring about its effect using the formula:

`De=B\cdot Da\,`

where *De* is the effective dose, *B* bioavailability and *Da* the administered dose.

Therefore, if a drug has a bioavailability of 0.8 (or 80%) and it is administered in a dose of 100 mg, the equation will demonstrate the following:

*De* = 0.8 × 100 mg = 80 mg

That is the 100 mg administered represents a blood plasma concentration of 80 mg that has the capacity to have a pharmaceutical effect.

This concept depends on a series of factors inherent to each drug, such as:

- Pharmaceutical form
- Chemical form
- Route of administration
- Stability
- Metabolism

These concepts, which are discussed in detail in their respective titled articles, can be mathematically quantified and integrated to obtain an overall mathematical equation:

`De=Q\cdot Da\cdot B\,`

where **Q** is the drug's purity.

`Va={\frac {Da\cdot B\cdot Q}{\tau }}`

where `Va` is the drug's rate of administration and `\tau` is the rate at which the absorbed drug reaches the circulatory system.

Finally, using the Henderson-Hasselbalch equation, and knowing the drug's `pKa\,`(pH at which there is an equilibrium between its ionized and non-ionized molecules), it is possible to calculate the non-ionized concentration of the drug and therefore the concentration that will be subject to absorption:

`\mathrm {pH} =\mathrm {pKa} +\log {\frac {B}{A}}`

When two drugs have the same bioavailability, they are said to be biological equivalents or bioequivalents. The concept of bioequivalence is important since it is currently used as a yardstick in the authorization of [[generic-drug|generic drugs]] in many countries.

## Analysis

### Bioanalytical methods

Bioanalytical methods are necessary to construct a concentration-time profile. Chemical techniques are employed to measure the concentration of drugs in biological matrix, most often plasma. Proper bioanalytical methods should be selective and sensitive. For example, microscale thermophoresis can be used to quantify how the biological matrix/liquid affects the affinity of a drug to its target.

### Mass spectrometry

Pharmacokinetics is often studied using mass spectrometry because of the complex nature of the matrix (often plasma or urine) and the need for high sensitivity to observe concentrations after a low dose and a long time period. The most common instrumentation used in this application is LC-MS with a triple quadrupole mass spectrometer. Tandem mass spectrometry is usually employed for added specificity. Standard curves and internal standards are used for quantitation of usually a single pharmaceutical in the samples. The samples represent different time points as a pharmaceutical is administered and then metabolized or cleared from the body. Blank samples taken before administration are important in determining background and ensuring data integrity with such complex sample matrices. Much attention is paid to the linearity of the standard curve; however it is common to use curve fitting with more complex functions such as quadratics since the response of most mass spectrometers is not linear across large concentration ranges.

There is currently considerable interest in the use of very high sensitivity mass spectrometry for microdosing studies, which are seen as a promising alternative to animal experimentation. Recent studies show that Secondary electrospray ionization (SESI-MS) can be used in drug monitoring, presenting the advantage of avoiding animal sacrifice.

## Population pharmacokinetics

*Population pharmacokinetics* (popPK) is the study of the sources and correlates of variability in drug concentrations among individuals who are the target patient population receiving clinically relevant doses of a drug of interest. Certain patient demographic, pathophysiological, and therapeutical features, such as body weight, excretory and metabolic functions, and the presence of other therapies, can regularly alter dose-concentration relationships and can explain variability in exposures. For example, steady-state concentrations of drugs eliminated mostly by the kidney are usually greater in patients with kidney failure than they are in patients with normal kidney function receiving the same drug dosage. Population pharmacokinetics seeks to identify the measurable pathophysiologic factors and explain sources of variability that cause changes in the dose-concentration relationship and the extent of these changes so that, if such changes are associated with clinically relevant and significant shifts in exposures that impact the [[therapeutic-index|therapeutic index]], dosage can be appropriately modified. Additionally, an advantage of population pharmacokinetic modelling is its ability to analyze sparse data sets (sometimes only one concentration measurement per patient is available).

## Clinical pharmacokinetics

| Antiepileptic medication | Cardioactive medication | Immunosuppressor medication | Antibiotic medication |
| --- | --- | --- | --- |
| Phenytoin Carbamazepine Valproic acid Lamotrigine Ethosuximide Phenobarbital Primidone | Digoxin Lidocaine | Ciclosporin Tacrolimus Sirolimus Everolimus Mycophenolate | Gentamicin Tobramycin Amikacin Vancomycin |
| Bronchodilator medication | Cytostatic medication | Antiviral (HIV) medication | Coagulation factors |
| Theophylline | Methotrexate 5-Fluorouracil Irinotecan | + Efavirenz Tenofovir Ritonavir | Factor VIII, Factor IX, Factor VIIa, Factor XI |

Clinical pharmacokinetics (arising from the clinical use of population pharmacokinetics) is the direct application to a therapeutic situation of knowledge regarding a drug's pharmacokinetics and the characteristics of a population that a patient belongs to (or can be ascribed to).

An example is the relaunch of the use of ciclosporin as an immunosuppressor to facilitate organ transplant. The drug's therapeutic properties were initially demonstrated, but it was almost never used after it was found to cause nephrotoxicity in a number of patients. However, it was then realized that it was possible to individualize a patient's dose of ciclosporin by analyzing the patients plasmatic concentrations (pharmacokinetic monitoring). This practice has allowed this drug to be used again and has facilitated a great number of organ transplants.

Clinical monitoring is usually carried out by determination of plasma concentrations as this data is usually the easiest to obtain and the most reliable. The main reasons for determining a drug's plasma concentration include:

- Narrow therapeutic range (difference between toxic and therapeutic concentrations)
- High toxicity
- High risk to life.

## Ecotoxicology

Ecotoxicology is the branch of science that deals with the nature, effects, and interactions of substances that are harmful to the environment such as microplastics and other biosphere harmful substances. Ecotoxicology is studied in pharmacokinetics due to the substances responsible for harming the environment such as pesticides can get into the bodies of living organisms. The health effects of these chemicals is thus subject to research and safety trials by government or international agencies such as the EPA or WHO. How long these chemicals stay in the body, the lethal dose and other factors are the main focus of Ecotoxicology.
